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
"""Tests for the validations module."""

import datetime
import json
import typing

import pandas as pd
import pytest

from adspace_agent.tools.cm360_trafficking import validations
from adspace_agent.tools.cm360_trafficking.trafficking import parse_sheet_tool

from .trafficking_test import make_test_csv
from .trafficking_test import mock_cm360_api_calls
from .trafficking_test import MockToolContext

_ = mock_cm360_api_calls
parse_trafficking_sheet = parse_sheet_tool

pytestmark = pytest.mark.usefixtures("mock_cm360_api_calls")


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
    row = pd.Series({"Profile ID": "12345"})
    assert validations.validate_profile_id(row, 1) is None

    row_empty = pd.Series({})
    assert (
        validations.validate_profile_id(row_empty, 1, profile_id="12345")
        is None
    )

    err = validations.validate_profile_id(row_empty, 1)
    assert err is not None
    assert err["field"] == "Profile ID"
    assert err["error"] == "Profile ID is required."


def test_validate_event_tags_all_branches() -> None:
    """Tests validations.validate_event_tags across all error paths."""
    assert validations.validate_event_tags(pd.Series({}), 1) == []

    err = validations.validate_event_tags(
        pd.Series({"Event Tag Types": "IMPRESSION"}), 1
    )
    assert len(err) == 1
    assert err[0]["field"] == "Event Tag Names"

    err_empty_names = validations.validate_event_tags(
        pd.Series({"Event Tag Names": " , "}), 1
    )
    assert len(err_empty_names) == 1
    assert "cannot be empty" in err_empty_names[0]["error"]

    err_missing_types_urls = validations.validate_event_tags(
        pd.Series({"Event Tag Names": "Tag1"}), 1
    )
    expected_err_count = 2
    assert len(err_missing_types_urls) == expected_err_count

    err_mismatch = validations.validate_event_tags(
        pd.Series({
            "Event Tag Names": "Tag1, Tag2",
            "Event Tag Types": "INVALID_TYPE",
            "Event Tag Urls": "https://a.com",
            "Event Tag Status": "INVALID_STATUS",
        }),
        1,
    )
    expected_mismatch_count = 5
    assert len(err_mismatch) == expected_mismatch_count

    valid = validations.validate_event_tags(
        pd.Series({
            "Event Tag Names": "Tag1",
            "Event Tag Types": "IMPRESSION_IMAGE_EVENT_TAG",
            "Event Tag Urls": "https://a.com",
            "Event Tag Status": "ENABLED",
        }),
        1,
    )
    assert valid == []
