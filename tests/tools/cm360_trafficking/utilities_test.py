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

import numpy as np

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
    assert utilities.format_date_time("2026-06-15") == "2026-06-15T00:00:00Z"
    assert utilities.format_date_time(np.nan) is None
    assert utilities.format_date_time("not-a-date") == "not-a-date"


def test_normalize_iso_datetime() -> None:
    """Tests normalize_iso_datetime normalizes timestamps to UTC ISO-8601."""
    assert (
        utilities.normalize_iso_datetime("2026-06-15T12:30:00Z")
        == "2026-06-15T12:30:00Z"
    )
    assert utilities.normalize_iso_datetime(None) is None
    assert utilities.normalize_iso_datetime("null") is None
    assert utilities.normalize_iso_datetime("invalid-date") == "invalid-date"
