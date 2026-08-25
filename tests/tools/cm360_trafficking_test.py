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

import datetime
import json
import pathlib
import typing
from unittest import mock

import pytest

from adspace_agent.tools.cm360_trafficking.cm360_trafficking import (
    _get_cm360_service,
)
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


class MockSession:
    def __init__(self, session_id: str = "test_session_id") -> None:
        self.id = session_id


@pytest.fixture(autouse=True)
def mock_cm360_api_calls(monkeypatch):
    """Mock CM360 API calls for placements and creatives to return matching mock data."""
    monkeypatch.setenv("SKILLS_BUCKET_NAME", "test-bucket")

    def mock_list_placements_side_effect(*args, **kwargs):
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

    def mock_list_creatives_side_effect(*args, **kwargs):
        return [
            {"name": "TEST_ACG~300x250", "id": "999123"},
            {"name": "TEST_Shopathon_300x250", "id": "999456"},
            {"name": "sap_elephant", "id": "999789"},
        ]

    def mock_list_event_tags_side_effect(*args, **kwargs):
        return []

    def mock_list_ads_side_effect(*args, **kwargs):
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

    def __init__(self, physical_path: str, session_id: str = "test_session_id") -> None:
        self.physical_path = physical_path
        self.session = MockSession(session_id)
        self.state: dict[str, typing.Any] = {}

    async def list_artifacts(self) -> list[str]:
        return [self.physical_path]

    async def load_artifact(self, filename: str):
        from google.genai import types

        path = pathlib.Path(self.physical_path)
        if not path.exists():
            return None

        if filename.lower().endswith(".csv"):
            text = path.read_text(encoding="utf-8")
            return types.Part(text=text)

        data = path.read_bytes()
        return types.Part(
            inline_data=types.Blob(
                mime_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                data=data,
            )
        )


def make_test_csv(
    profile_id: str = "7023449",
    advertiser_id: str = "13641571",
    campaign_id: str = "30535365",
    campaign_name: str = "Google Ad Spaces Testing - June 2026",
    status: str = "New",
    site_id: str = "4802860",
    site: str = "",
    placement_id: str = "",
    placement_name: str = "Test~Placement~1 Test",
    placement_size: str = "300x250",
    compatibility: str = "DISPLAY",
    payment_source: str = "PLACEMENT_AGENCY_PAID",
    pricing_type: str = "PRICING_TYPE_CPM",
    pricing_start: str = "8/29/3000",
    pricing_end: str = "9/25/3000",
    tag_formats: str = "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT",
    ad_id: str = "",
    ad_name: str = "Test Ad C",
    ad_start: str = "8/29/3000",
    ad_end: str = "9/25/3000",
    ad_type: str = "AD_SERVING_STANDARD_AD",
    priority: str = "AD_PRIORITY_01",
    impression_ratio: str = "1",
    creative_id: str = "999123",
    creative_name: str = "TEST_ACG~300x250",
    click_through_url: str = "https://click.test.com",
    ad_dynamic_click_tracker: str = "TRUE",
    base_url: str = "https://www.test.com",
    final_url: str = "https://www.test.com?utm_medium=test",
    placement_status: str = "PLACEMENT_STATUS_ACTIVE",
) -> str:
    """Helper to construct a trafficking sheet CSV string with defaults."""
    r1 = (
        "Digitas-DFA 609,Profile ID,Advertiser ID,Campaign ID,Campaign Name"
        + "," * 41
        + "\n"
    )
    r2 = (
        f",{profile_id},{advertiser_id},{campaign_id},{campaign_name}" + "," * 41 + "\n"
    )
    r3 = (
        "," * 9
        + "Placement Type Not in API - renamed to Compatibility"
        + "," * 36
        + "\n"
    )
    r4 = "," * 45 + "\n"
    r5 = (
        "Campaign ID,Campaign Name,Trafficking Status,Site ID,Site,Channel,Placement ID,Placement Name,"
        "Placement Size,Compatibility,Placement Status,Placement Payment Source,Pricing Schedule Type,"
        "Pricing Schedule Start Date,Pricing Schedule End Date,Placement Tag Formats,Ad Server,Ad ID,Ad Name,"
        "Ad Start Date,Ad End Date,Ad Type,Delivery Schedule Priority,Delivery Schedule Impression Ratio,"
        "Ad Click Through Url,Ad Dynamic Click Tracker,Creative ID,Creative Name,Creative Start Date,"
        "Creative End Date,Creative Rotation,Creative Dimensions,Creative Type,Creative asset file link,"
        "Base URL,Final Trafficking URL,Brand Safety/Verification,Brand Safety/Verification Measurement Type,"
        "Research Partner,Notes,Campaign Funding,Campaign Quarter,Creative Detail,Fiscal,Language,Salesforce ID\n"
    )
    r6 = (
        f"{campaign_id},{campaign_name},{status},{site_id},{site},display,{placement_id},{placement_name},{placement_size},"
        f"{compatibility},{placement_status},{payment_source},{pricing_type},{pricing_start},{pricing_end},"
        f'"{tag_formats}",DCM,{ad_id},{ad_name},{ad_start},{ad_end},{ad_type},{priority},{impression_ratio},'
        f"{click_through_url},{ad_dynamic_click_tracker},{creative_id},{creative_name},6/10/2026,7/10/2026,100%,300x250,HTML5_BANNER,,"
        f"{base_url},{final_url},None,None,CINT,,,,,,,\n"
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
        (op["payload"] for op in ad_ops if op["payload"]["name"] == "Test Ad C"),
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

    assert result.get("status") in ("error", "ERROR")


@pytest.mark.asyncio
async def test_cm360_trafficking_parser_toolset() -> None:
    """Tests that the parser toolset returns the correct tools."""
    toolset = CM360TraffickingParserToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 2
    assert tools[0] is toolset.parse_sheet_tool
    assert tools[1] is toolset.traffic_campaigns_in_cm360_tool


def test_validate_placement_id() -> None:
    """Tests validate_placement_id helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_placement_id,
    )

    row_update_missing = pd.Series(
        {
            "Trafficking Status": "Update",
            "Placement ID": "",
        }
    )
    err = validate_placement_id(row_update_missing, 1)
    assert err is not None
    assert err["field"] == "Placement ID"
    assert "Placement ID is required" in err["error"]

    row_update_valid = pd.Series(
        {
            "Trafficking Status": "Update",
            "Placement ID": "12345",
        }
    )
    assert validate_placement_id(row_update_valid, 1) is None

    row_new_missing = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement ID": "",
        }
    )
    assert validate_placement_id(row_new_missing, 1) is None


@pytest.mark.asyncio
async def test_validation_success_with_placement_id_on_update(
    tmp_path,
) -> None:
    """Tests that parsing succeeds when Placement ID is provided on Update."""
    csv_content = make_test_csv(status="Update", placement_id="123456", ad_id="9999")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "SUCCESS"
    assert "operations" in result

    ad_ops = [
        op
        for op in result["operations"]
        if op["operation"] == "dfareporting.ads.insert"
    ]
    assert len(ad_ops) == 1


def test_validate_placement_name() -> None:
    """Tests validate_placement_name helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_placement_name,
    )

    row_missing = pd.Series({"Trafficking Status": "New", "Placement Name": ""})
    err = validate_placement_name(row_missing, 1)
    assert err is not None
    assert err["field"] == "Placement Name"
    assert "Placement Name is required" in err["error"]

    row_long = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Name": "A" * 513,
        }
    )
    err = validate_placement_name(row_long, 1)
    assert err is not None
    assert "must be less than or equal to 512 characters" in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Name": "Valid Name",
        }
    )
    assert validate_placement_name(row_valid, 1) is None


@pytest.mark.asyncio
async def test_validation_fail_missing_profile_id(
    tmp_path,
) -> None:
    """Tests that validation fails when Profile ID is missing."""
    csv_content = make_test_csv(profile_id="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") in ("error", "ERROR")
    assert "Profile ID" in result.get("message", "")


@pytest.mark.asyncio
async def test_validation_fail_missing_advertiser_id(
    tmp_path,
) -> None:
    """Tests that validation fails when Advertiser ID is missing."""
    csv_content = make_test_csv(advertiser_id="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") in ("error", "ERROR")
    assert "Advertiser ID" in result.get("message", "")


@pytest.mark.asyncio
async def test_validation_fail_missing_campaign_id(
    tmp_path,
) -> None:
    """Tests that validation fails when Campaign ID is missing."""
    csv_content = make_test_csv(campaign_id="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") in ("error", "ERROR")
    assert "Campaign ID" in result.get("message", "")


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


def test_validate_site_id() -> None:
    """Tests validate_site_id helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_site_id,
    )

    row_missing = pd.Series({"Site ID": ""})
    err = validate_site_id(row_missing, 1)
    assert err is not None
    assert err["field"] == "Site ID"
    assert "Site ID is required" in err["error"]

    row_valid_id = pd.Series({"Site ID": "12345"})
    assert validate_site_id(row_valid_id, 1) is None


def test_validate_payment_source() -> None:
    """Tests validate_payment_source helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_payment_source,
    )

    row_missing = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Payment Source": "",
        }
    )
    err = validate_payment_source(row_missing, 1)
    assert err is not None
    assert err["field"] == "Payment Source"
    assert "Payment Source is required" in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Payment Source": "PLACEMENT_AGENCY_PAID",
        }
    )
    assert validate_payment_source(row_valid, 1) is None


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


def test_validate_compatibility() -> None:
    """Tests validate_compatibility helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_compatibility,
    )

    row_missing = pd.Series({"Trafficking Status": "New", "Compatibility": ""})
    err = validate_compatibility(row_missing, 1)
    assert err is not None
    assert "Compatibility is required" in err["error"]

    row_forbidden = pd.Series(
        {
            "Trafficking Status": "New",
            "Compatibility": "APP",
        }
    )
    err = validate_compatibility(row_forbidden, 1)
    assert err is not None
    assert "APP and APP_INTERSTITIAL are no longer allowed" in err["error"]

    row_invalid = pd.Series(
        {
            "Trafficking Status": "New",
            "Compatibility": "INVALID",
        }
    )
    err = validate_compatibility(row_invalid, 1)
    assert err is not None
    assert "Invalid compatibility value" in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Compatibility": "DISPLAY",
        }
    )
    assert validate_compatibility(row_valid, 1) is None


def test_validate_placement_size() -> None:
    """Tests validate_placement_size helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_placement_size,
    )

    row_missing = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Size": "",
        }
    )
    err = validate_placement_size(row_missing, 1)
    assert err is not None
    assert "Placement Size is required" in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Size": "300x250",
        }
    )
    assert validate_placement_size(row_valid, 1) is None


def test_validate_pricing_schedule() -> None:
    """Tests validate_pricing_schedule helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_pricing_schedule,
    )

    row_missing = pd.Series(
        {
            "Trafficking Status": "New",
            "Pricing Schedule Start Date": "",
            "Pricing Schedule End Date": "9/25/3000",
            "Pricing Schedule Type": "PRICING_TYPE_CPM",
        }
    )
    err = validate_pricing_schedule(row_missing, 1)
    assert err is not None
    assert "Pricing Schedule subfields (startDate) are required" in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Pricing Schedule Start Date": "8/29/3000",
            "Pricing Schedule End Date": "9/25/3000",
            "Pricing Schedule Type": "PRICING_TYPE_CPM",
        }
    )
    assert validate_pricing_schedule(row_valid, 1) is None


def test_validate_tag_formats() -> None:
    """Tests validate_tag_formats helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_tag_formats,
    )

    row_missing = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Tag Formats": "",
        }
    )
    err = validate_tag_formats(row_missing, 1)
    assert err is not None
    assert "Placement Tag Formats is required" in err["error"]

    row_invalid = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Tag Formats": ("PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_INVALID"),
        }
    )
    err = validate_tag_formats(row_invalid, 1)
    assert err is not None
    assert "Invalid placement tag format values: PLACEMENT_TAG_INVALID." in err["error"]

    row_valid = pd.Series(
        {
            "Trafficking Status": "New",
            "Placement Tag Formats": (
                "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT"
            ),
        }
    )
    assert validate_tag_formats(row_valid, 1) is None


def test_validate_ad_id() -> None:
    """Tests validate_ad_id helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import validate_ad_id

    row_missing = pd.Series({"Trafficking Status": "Update", "Ad ID": ""})
    err = validate_ad_id(row_missing, 1)
    assert err is not None
    assert err["field"] == "Ad ID"
    assert "Ad ID is required" in err["error"]

    row_valid = pd.Series({"Trafficking Status": "Update", "Ad ID": "12345"})
    assert validate_ad_id(row_valid, 1) is None


@pytest.mark.asyncio
async def test_validation_fail_missing_ad_name_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when Trafficking Status is New but Ad Name is missing."""
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad Name"
    assert "Ad Name is required" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_long_ad_name_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when Trafficking Status is New and Ad Name > 256 characters."""
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad Name"
    assert "Ad Name must be less than or equal to 256 characters" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_ad_start_time_past(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Start Date is in the past on New."""
    past_date = (datetime.date.today() - datetime.timedelta(days=1)).strftime(
        "%Y-%m-%d"
    )
    csv_content = make_test_csv(ad_start=past_date)
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad Start Date"
    assert "cannot be in the past" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_pass_ad_start_time_today(
    tmp_path,
) -> None:
    """Tests that validation passes when Ad Start Date is today."""
    today_date = datetime.date.today().strftime("%Y-%m-%d")
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
async def test_validation_fail_ad_end_time_not_later(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad End Date is not later than Ad Start Date."""
    csv_content = make_test_csv(ad_start="8/29/3000", ad_end="8/28/3000")
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad End Date"
    assert "must be later than Ad Start Date" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_ad_invalid_date(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Start Date is not a valid date format."""
    csv_content = make_test_csv(ad_start="invalid-date")
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad Start Date"
    assert "is not a valid date" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_ad_start_time_past_on_update(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Start Date is in the past even on Update status."""
    past_date = (datetime.date.today() - datetime.timedelta(days=1)).strftime(
        "%Y-%m-%d"
    )
    csv_content = make_test_csv(
        status="Update", ad_start=past_date, ad_id="9999", placement_id="8888"
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad Start Date"
    assert "cannot be in the past" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_ad_end_time_not_later_on_update(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad End Date is not later than Ad Start Date on Update status."""
    csv_content = make_test_csv(
        status="Update",
        ad_start="8/29/3000",
        ad_end="8/28/3000",
        ad_id="9999",
        placement_id="8888",
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
    assert errors[0]["row"] == 6
    assert errors[0]["field"] == "Ad End Date"
    assert "must be later than Ad Start Date" in errors[0]["error"]


def test_validate_placement_assignment() -> None:
    """Tests validate_placement_assignment helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_placement_assignment,
    )

    row_missing = pd.Series({"Placement ID": "", "Placement Name": ""})
    err = validate_placement_assignment(row_missing, 1)
    assert err is not None
    assert err["field"] == "Placement ID"
    assert "Placement ID is required" in err["error"]

    row_valid_name = pd.Series({"Placement Name": "Test Placement"})
    assert validate_placement_assignment(row_valid_name, 1) is None

    row_valid_id = pd.Series({"Placement ID": "12345"})
    assert validate_placement_assignment(row_valid_id, 1) is None


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
    """Tests that validation fails when forbidden AD_SERVING_DEFAULT_AD type is used on New."""
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
async def test_validation_fail_missing_delivery_schedule_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when delivery schedule priority is missing on standard ad creation."""
    csv_content = make_test_csv(ad_type="AD_SERVING_STANDARD_AD", priority="")
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
    schedule_error = next(e for e in errors if e["field"] == "Delivery Schedule")
    assert "required when Ad Type is AD_SERVING_STANDARD_AD" in schedule_error["error"]


@pytest.mark.asyncio
async def test_validation_fail_missing_delivery_schedule_on_update(
    tmp_path,
) -> None:
    """Tests that validation fails when priority is missing on standard ad update."""
    csv_content = make_test_csv(
        status="Update",
        ad_type="AD_SERVING_STANDARD_AD",
        priority="",
        impression_ratio="1",
        ad_id="9999",
        placement_id="8888",
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
    schedule_error = next(e for e in errors if e["field"] == "Delivery Schedule")
    assert "required when Ad Type is AD_SERVING_STANDARD_AD" in schedule_error["error"]


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
    """Tests that validation fails when Final Trafficking URL is missing for creative assignment."""
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
    """Tests validate_placement_status helper."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_placement_status,
    )

    row_invalid = pd.Series({"Placement Status": "PLACEMENT_STATUS_INVALID"})
    err = validate_placement_status(row_invalid, 1)
    assert err is not None
    assert err["field"] == "Placement Status"
    assert "Invalid placement status 'PLACEMENT_STATUS_INVALID'" in err["error"]

    row_valid = pd.Series({"Placement Status": "PLACEMENT_STATUS_ACTIVE"})
    assert validate_placement_status(row_valid, 1) is None


def test_group_placements_status_mapping() -> None:
    """Tests that a valid Placement Status maps correctly to activeStatus in grouped placements."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.cm360_actions import (
        _group_placements,
    )

    df = pd.DataFrame(
        [
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
        ]
    )
    placements = _group_placements(df, advertiser_id="123", campaign_id="456")
    assert "Test Placement 1" in placements
    assert placements["Test Placement 1"]["activeStatus"] == "PLACEMENT_STATUS_INACTIVE"


def test_resolve_placement_ids() -> None:
    """Tests _resolve_placement_ids correctly resolves placement names to CM360 IDs."""
    from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
        _resolve_placement_ids,
    )

    ads = {
        "Test Ad": {
            "name": "Test Ad",
            "placementAssignments": [
                {"placementId": "Test~Placement~1 Test", "active": True}
            ],
        }
    }
    _resolve_placement_ids(
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
    """Tests _resolve_creative_ids correctly resolves creative names to CM360 IDs."""
    from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
        _resolve_creative_ids,
    )

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
    _resolve_creative_ids(
        ads=ads,
        profile_id="7023449",
        advertiser_id="13641571",
        campaign_id="30535365",
        tool_context=None,
    )
    assert (
        ads["Test Ad"]["creativeRotation"]["creativeAssignments"][0]["creativeId"]
        == "999123"
    )


def test_get_cm360_service_success_from_dict() -> None:
    """Tests _get_cm360_service creates service when valid credentials dict is in state."""
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
        service = _get_cm360_service(mock_context)
        assert service == mock_build.return_value
        mock_build.assert_called_once()
        args, kwargs = mock_build.call_args
        assert args == ("dfareporting", "v5")
        assert kwargs["credentials"].token == "ya29.mock_token"


def test_get_cm360_service_success_from_credentials_object() -> None:
    """Tests _get_cm360_service uses credentials object when already present in state."""
    from google.oauth2.credentials import Credentials

    mock_credentials = mock.MagicMock(spec=Credentials)
    mock_context = mock.MagicMock()
    mock_context.state = {CREDENTIALS_CACHE_KEY: mock_credentials}

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.cm360_actions.build"
    ) as mock_build:
        mock_build.return_value = mock.MagicMock()
        service = _get_cm360_service(mock_context)
        assert service == mock_build.return_value
        mock_build.assert_called_once_with(
            "dfareporting", "v5", credentials=mock_credentials
        )


def test_get_cm360_service_missing_tool_context() -> None:
    """Tests _get_cm360_service raises ValueError when tool_context is None."""
    with pytest.raises(
        ValueError,
        match="Tool context and state are required to get CM360 service.",
    ):
        _get_cm360_service(None)


def test_get_cm360_service_missing_state() -> None:
    """Tests _get_cm360_service raises ValueError when tool_context.state is None."""
    mock_context = mock.MagicMock()
    mock_context.state = None
    with pytest.raises(
        ValueError,
        match="Tool context and state are required to get CM360 service.",
    ):
        _get_cm360_service(mock_context)


def test_get_cm360_service_missing_credentials_key() -> None:
    """Tests _get_cm360_service raises ValueError when CREDENTIALS_CACHE_KEY is missing."""
    mock_context = mock.MagicMock()
    mock_context.state = {}
    with pytest.raises(ValueError, match="Credentials not found in tool context state"):
        _get_cm360_service(mock_context)


def test_before_traffic_campaigns_in_cm360_tool_callback_valid_cached_credentials() -> (
    None
):
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


def test_before_traffic_campaigns_in_cm360_tool_callback_refresh_expired_credentials() -> (
    None
):
    """Tests before callback when cached credentials are expired and refreshed."""
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
    mock_credentials_obj.refresh_token = "mock_refresh"
    mock_credentials_obj.to_json.return_value = json.dumps(
        {"token": "ya29.refreshed_token"}
    )

    def on_refresh(*args, **kwargs):
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


def test_before_traffic_campaigns_in_cm360_tool_callback_exchanged_credentials() -> (
    None
):
    """Tests before callback when credentials are exchanged successfully."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "traffic_campaigns_in_cm360_tool"
    mock_context = mock.MagicMock()
    mock_context.state = {}

    mock_oauth = mock.MagicMock()
    mock_oauth.access_token = "ya29.exchanged_token"
    mock_oauth.refresh_token = "mock_exchanged_refresh"

    mock_auth_response = mock.MagicMock()
    mock_auth_response.oauth2 = mock_oauth
    mock_context.get_auth_response.return_value = mock_auth_response

    res = before_traffic_campaigns_in_cm360_tool_callback(
        tool=mock_tool, args={}, tool_context=mock_context
    )
    assert res is None
    assert mock_context.state[CREDENTIALS_CACHE_KEY]["token"] == "ya29.exchanged_token"
    assert (
        mock_context.state[CREDENTIALS_CACHE_KEY]["refresh_token"]
        == "mock_exchanged_refresh"
    )


def test_before_traffic_campaigns_in_cm360_tool_callback_request_credential() -> None:
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


def test_before_traffic_campaigns_in_cm360_tool_callback_unrelated_tool() -> None:
    """Tests before callback ignores unrelated tools."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "unrelated_tool"
    mock_context = mock.MagicMock()

    res = before_traffic_campaigns_in_cm360_tool_callback(
        tool=mock_tool, args={}, tool_context=mock_context
    )
    assert res is None


@pytest.mark.asyncio
async def test_validation_click_tracker_success_new(tmp_path) -> None:
    """Tests that AD_SERVING_CLICK_TRACKER with TRUE passes on New status."""
    csv_content = make_test_csv(
        ad_type="AD_SERVING_CLICK_TRACKER",
        ad_dynamic_click_tracker="TRUE",
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
    assert ad_ops[0]["payload"]["dynamicClickTracker"] is True
    assert "deliverySchedule" not in ad_ops[0]["payload"]


@pytest.mark.asyncio
async def test_validation_click_tracker_success_update(tmp_path) -> None:
    """Tests that AD_SERVING_CLICK_TRACKER with FALSE passes on Update status."""
    csv_content = make_test_csv(
        status="Update",
        ad_id="123456",
        placement_id="789101",
        ad_type="AD_SERVING_CLICK_TRACKER",
        ad_dynamic_click_tracker="FALSE",
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
    assert ad_ops[0]["payload"]["dynamicClickTracker"] is False
    assert "deliverySchedule" not in ad_ops[0]["payload"]


@pytest.mark.asyncio
async def test_validation_click_tracker_fail_missing_value_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Dynamic Click Tracker is missing on New."""
    csv_content = make_test_csv(
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
    assert "Ad Dynamic Click Tracker is required" in click_tracker_error["error"]


@pytest.mark.asyncio
async def test_validation_click_tracker_fail_missing_value_on_update(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Dynamic Click Tracker is missing on Update."""
    csv_content = make_test_csv(
        status="Update",
        ad_id="123456",
        placement_id="789101",
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
    assert "Ad Dynamic Click Tracker is required" in click_tracker_error["error"]


@pytest.mark.asyncio
async def test_validation_click_tracker_fail_invalid_boolean(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Dynamic Click Tracker is not TRUE or FALSE."""
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
    """Tests validate_profile_id with various inputs."""
    import pandas as pd

    from adspace_agent.tools.cm360_trafficking.validations import (
        validate_profile_id,
    )

    # Test with Profile ID in row
    row = pd.Series({"Profile ID": "12345"})
    assert validate_profile_id(row, 1) is None

    # Test with Profile ID from parameter fallback
    row_empty = pd.Series({})
    assert validate_profile_id(row_empty, 1, profile_id="12345") is None

    # Test with missing Profile ID
    err = validate_profile_id(row_empty, 1)
    assert err is not None
    assert err["field"] == "Profile ID"
    assert err["error"] == "Profile ID is required."


def test_resolve_and_build_operations_with_existing_entities() -> None:
    """Tests _resolve_and_build_operations resolves update vs insert operations and existing IDs."""
    from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
        _resolve_and_build_operations,
    )

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
        ops = _resolve_and_build_operations(
            ads=ads,
            event_tags=event_tags,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )

    # 2 event tag operations + 2 ad operations
    assert len(ops) == 4

    # Event tags
    existing_tag_op = next(op for op in ops if op["name"] == "Existing Tag")
    assert existing_tag_op["operation"] == "dfareporting.eventTags.update"
    assert existing_tag_op["payload"]["id"] == "tag_999"

    new_tag_op = next(op for op in ops if op["name"] == "New Tag")
    assert new_tag_op["operation"] == "dfareporting.eventTags.insert"
    assert "id" not in new_tag_op["payload"]

    # Ads
    existing_ad_op = next(op for op in ops if op["name"] == "Existing Ad")
    assert existing_ad_op["operation"] == "dfareporting.ads.update"
    assert existing_ad_op["payload"]["id"] == "ad_888"
    # Merged placements: mock_placement_1 (from CM360) + mock_placement_2 (from sheet)
    placement_ids = [
        p["placementId"] for p in existing_ad_op["payload"]["placementAssignments"]
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
    from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
        _extract_assigned_placement_ids,
    )

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
    extracted_dict = _extract_assigned_placement_ids(ads)
    assert extracted_dict == ["p1", "p2", "p3"]
