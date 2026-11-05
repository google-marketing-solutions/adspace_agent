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
"""Tests for the cm360_trafficking utilities module."""

from unittest import mock
from zoneinfo import ZoneInfoNotFoundError

import numpy as np
import pandas as pd
import pytest

from adspace_agent.tools.cm360_trafficking import utilities


def test_parse_size() -> None:
    """Tests parse_size parses valid dimensions and handles invalid inputs."""
    assert utilities.parse_size("300x250") == {"width": 300, "height": 250}
    assert utilities.parse_size(" 728 x 90 ") == {"width": 728, "height": 90}
    assert utilities.parse_size(np.nan) == {"width": 0, "height": 0}
    assert utilities.parse_size("invalid") == {"width": 0, "height": 0}
    assert utilities.parse_size("axb") == {"width": 0, "height": 0}


def test_format_date() -> None:
    """Tests format_date converts dates and handles missing/invalid inputs."""
    assert utilities.format_date("2026-06-15") == "2026-06-15"
    assert utilities.format_date("6/15/2026") == "2026-06-15"
    assert utilities.format_date(np.nan) is None
    assert utilities.format_date("not-a-date extra") == "not-a-date"


def test_format_date_time() -> None:
    """Tests format_date_time converts datetimes to ISO-8601 strings."""
    assert (
        utilities.format_date_time("2026-06-15", time_zone="UTC")
        == "2026-06-15T00:00:00Z"
    )
    assert utilities.format_date_time(np.nan, time_zone="UTC") is None
    assert (
        utilities.format_date_time("not-a-date", time_zone="UTC")
        == "not-a-date"
    )
    assert (
        utilities.format_date_time("11/03/2026", time_zone="America/New_York")
        == "2026-11-03T05:00:00Z"
    )
    assert (
        utilities.format_date_time(
            "11/20/2026", time_zone="America/New_York", is_end_date=True
        )
        == "2026-11-21T04:59:59Z"
    )
    assert (
        utilities.format_date_time(
            "2026-10-16T13:29:48.910Z", time_zone="America/New_York"
        )
        == "2026-10-16T13:29:48Z"
    )
    with pytest.raises(ValueError, match="Time zone must be provided"):
        utilities.format_date_time("11/03/2026", time_zone="")
    with pytest.raises(ZoneInfoNotFoundError):
        utilities.format_date_time("11/03/2026", time_zone="Invalid/Zone")


def test_normalize_iso_datetime() -> None:
    """Tests normalize_iso_datetime normalizes timestamps to UTC ISO-8601."""
    assert (
        utilities.normalize_iso_datetime("2026-06-15T12:30:00Z")
        == "2026-06-15T12:30:00Z"
    )
    assert utilities.normalize_iso_datetime(None) is None
    assert utilities.normalize_iso_datetime("null") is None
    assert utilities.normalize_iso_datetime("invalid-date") == "invalid-date"


def test_prepare_data_with_timezone() -> None:
    """Tests prepare_data extracts metadata and Time Zone correctly."""
    raw_data = [
        [
            "Network",
            "Profile ID",
            "Advertiser ID",
            "Campaign ID",
            "Campaign Name",
            "Time Zone",
        ],
        [
            "Digitas",
            "12345",
            "67890",
            "99999",
            "Test Campaign",
            "America/Chicago",
        ],
        [
            "Trafficking Status",
            "Placement Name",
            "Ad Name",
            "Placement ID",
            "Ad ID",
        ],
        ["New", "Placement 1", "Ad 1", "", ""],
    ]
    df_raw = pd.DataFrame(raw_data)
    df, profile_id, advertiser_id, campaign_id, campaign_name = (
        utilities.prepare_data(df_raw)
    )

    assert profile_id == "12345"
    assert advertiser_id == "67890"
    assert campaign_id == "99999"
    assert campaign_name == "Test Campaign"
    assert "Time Zone" in df.columns
    assert df["Time Zone"].iloc[0] == "America/Chicago"


def test_prepare_data_missing_timezone() -> None:
    """Tests prepare_data raises ValueError when Time Zone is missing."""
    raw_data = [
        [
            "Network",
            "Profile ID",
            "Advertiser ID",
            "Campaign ID",
            "Campaign Name",
        ],
        [
            "Digitas",
            "12345",
            "67890",
            "99999",
            "Test Campaign",
        ],
        [
            "Trafficking Status",
            "Placement Name",
            "Ad Name",
            "Placement ID",
            "Ad ID",
        ],
        ["New", "Placement 1", "Ad 1", "", ""],
    ]
    df_raw = pd.DataFrame(raw_data)
    with pytest.raises(
        ValueError, match="Missing required campaign metadata field: Time Zone"
    ):
        utilities.prepare_data(df_raw)


def test_prepare_data_invalid_timezone() -> None:
    """Tests prepare_data raises ValueError when Time Zone is invalid."""
    raw_data = [
        [
            "Network",
            "Profile ID",
            "Advertiser ID",
            "Campaign ID",
            "Campaign Name",
            "Time Zone",
        ],
        [
            "Digitas",
            "12345",
            "67890",
            "99999",
            "Test Campaign",
            "Invalid/Time_Zone",
        ],
        [
            "Trafficking Status",
            "Placement Name",
            "Ad Name",
            "Placement ID",
            "Ad ID",
        ],
        ["New", "Placement 1", "Ad 1", "", ""],
    ]
    df_raw = pd.DataFrame(raw_data)
    with pytest.raises(ValueError, match="Invalid Time Zone"):
        utilities.prepare_data(df_raw)


def test_clean_dataframe_status_filtering() -> None:
    """Tests clean_dataframe retains New, Update, Partially_failed, Failed."""
    raw_data = [
        ["Meta1", "Val1", "", "", ""],
        ["Meta2", "Val2", "", "", ""],
        [
            "Trafficking Status",
            "Placement Name",
            "Ad Name",
            "Placement ID",
            "Ad ID",
        ],
        ["New", "P1", "Ad1", "", ""],
        ["Update", "P2", "Ad2", "123", "456"],
        ["Partially_failed", "P3", "Ad3", "", ""],
        ["Failed", "P4", "Ad4", "", ""],
        ["Trafficked", "P5", "Ad5", "789", "101"],
        ["OtherStatus", "P6", "Ad6", "", ""],
    ]
    df_raw = pd.DataFrame(raw_data)
    cleaned_df = utilities.clean_dataframe(df_raw)

    statuses = cleaned_df["Trafficking Status"].tolist()
    assert statuses == ["New", "Update", "Partially_failed", "Failed"]


@pytest.mark.asyncio
async def test_get_latest_csv_artifact_single_file() -> None:
    """Tests get_latest_csv_artifact immediately returns single match."""
    mock_context = mock.MagicMock()
    result = await utilities.get_latest_csv_artifact(
        mock_context, ["single.csv"]
    )
    assert result == "single.csv"


@pytest.mark.asyncio
async def test_get_latest_csv_artifact_session_events() -> None:
    """Tests get_latest_csv_artifact discovers latest file from events."""
    mock_context = mock.MagicMock()
    event1 = mock.MagicMock()
    event1.content.parts = [
        mock.MagicMock(
            function_call=mock.MagicMock(
                name="load_artifacts",
                args={"artifact_names": ["first_sheet.csv"]},
            ),
            text=None,
        )
    ]
    event2 = mock.MagicMock()
    event2.content.parts = [
        mock.MagicMock(
            function_call=None,
            text='[Uploaded Artifact: "my_arbitrary_name.csv"]',
        )
    ]
    mock_context.session.events = [event1, event2]

    result = await utilities.get_latest_csv_artifact(
        mock_context, ["first_sheet.csv", "my_arbitrary_name.csv", "old.csv"]
    )
    assert result == "my_arbitrary_name.csv"


@pytest.mark.asyncio
async def test_get_latest_csv_artifact_by_timestamp(tmp_path) -> None:
    """Tests get_latest_csv_artifact orders by timestamp when events empty."""
    file_old = tmp_path / "older.csv"
    file_old.write_text("a,b")
    file_new = tmp_path / "newest.csv"
    file_new.write_text("a,b")

    mock_context = mock.MagicMock()
    mock_context.session.events = []

    ver_old = mock.MagicMock()
    ver_old.canonical_uri = file_old.as_uri()

    ver_new = mock.MagicMock()
    ver_new.canonical_uri = file_new.as_uri()

    def mock_version(filename: str) -> mock.MagicMock:
        return ver_new if filename == "newest.csv" else ver_old

    mock_context.get_artifact_version = mock.AsyncMock(side_effect=mock_version)

    result = await utilities.get_latest_csv_artifact(
        mock_context, ["older.csv", "newest.csv"]
    )
    assert result == "newest.csv"


@pytest.mark.asyncio
async def test_load_raw_dataframe_latest_upload() -> None:
    """Tests load_raw_dataframe selects the latest uploaded artifact."""
    mock_context = mock.MagicMock()
    mock_context.list_artifacts = mock.AsyncMock(
        return_value=["first.csv", "second.csv"]
    )
    event = mock.MagicMock()
    event.content.parts = [
        mock.MagicMock(
            function_call=None,
            text='[Uploaded Artifact: "second.csv"]',
        )
    ]
    mock_context.session.events = [event]
    mock_part = mock.MagicMock()
    mock_part.text = "col1,col2\nval1,val2\n"
    mock_part.inline_data = None
    mock_context.load_artifact = mock.AsyncMock(return_value=mock_part)

    df, filename = await utilities.load_raw_dataframe(mock_context)
    assert filename == "second.csv"
    assert not df.empty
    mock_context.load_artifact.assert_called_once_with("second.csv")


@pytest.mark.asyncio
async def test_load_raw_dataframe_explicit_filename() -> None:
    """Tests load_raw_dataframe honors explicit target_filename."""
    mock_context = mock.MagicMock()
    mock_context.list_artifacts = mock.AsyncMock(
        return_value=[
            "TSheet (10).csv",
            "TSheet (8).csv",
        ]
    )
    mock_part = mock.MagicMock()
    mock_part.text = "col1,col2\nval1,val2\n"
    mock_part.inline_data = None
    mock_context.load_artifact = mock.AsyncMock(return_value=mock_part)

    _, filename = await utilities.load_raw_dataframe(
        mock_context, target_filename="TSheet (8).csv"
    )
    assert filename == "TSheet (8).csv"
    mock_context.load_artifact.assert_called_once_with("TSheet (8).csv")
