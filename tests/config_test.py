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
"""Testing the config module."""

from unittest.mock import patch

import pytest

import adspace_agent.config as config_module


def test_get_google_ads_tool_filter_variants():
    """Test get_google_ads_tool_filter with various env values."""
    with patch.dict("os.environ", {}, clear=False):
        config_module.os.environ.pop("GOOGLE_ADS_TOOL_FILTER", None)
        assert (
            config_module.get_google_ads_tool_filter()
            == config_module.DEFAULT_GOOGLE_ADS_TOOL_FILTER
        )

    with patch.dict("os.environ", {"GOOGLE_ADS_TOOL_FILTER": " , "}):
        assert (
            config_module.get_google_ads_tool_filter()
            == config_module.DEFAULT_GOOGLE_ADS_TOOL_FILTER
        )

    with patch.dict("os.environ", {"GOOGLE_ADS_TOOL_FILTER": "*"}):
        assert config_module.get_google_ads_tool_filter() is None

    with patch.dict("os.environ", {"GOOGLE_ADS_TOOL_FILTER": "ALL"}):
        assert config_module.get_google_ads_tool_filter() is None

    with patch.dict("os.environ", {"GOOGLE_ADS_TOOL_FILTER": "none"}):
        assert config_module.get_google_ads_tool_filter() == []

    with patch.dict(
        "os.environ", {"GOOGLE_ADS_TOOL_FILTER": " tool_a , tool_b "}
    ):
        assert config_module.get_google_ads_tool_filter() == [
            "tool_a",
            "tool_b",
        ]


def test_get_enabled_toolsets_variants():
    """Test get_enabled_toolsets with default, empty, and invalid values."""
    with patch.dict("os.environ", {}, clear=False):
        config_module.os.environ.pop("ENABLED_TOOLSETS", None)
        assert (
            config_module.get_enabled_toolsets()
            == config_module.ALL_GOOGLE_TOOLSETS
        )

    with patch.dict("os.environ", {"ENABLED_TOOLSETS": "   "}):
        assert (
            config_module.get_enabled_toolsets()
            == config_module.ALL_GOOGLE_TOOLSETS
        )

    with patch.dict("os.environ", {"ENABLED_TOOLSETS": "all"}):
        assert (
            config_module.get_enabled_toolsets()
            == config_module.ALL_GOOGLE_TOOLSETS
        )

    with patch.dict("os.environ", {"ENABLED_TOOLSETS": "none"}):
        assert config_module.get_enabled_toolsets() == []

    with (
        patch.dict("os.environ", {"ENABLED_TOOLSETS": "google-ads,dv360"}),
        pytest.raises(
            ValueError, match=r"Unknown toolset\(s\) in ENABLED_TOOLSETS"
        ),
    ):
        config_module.get_enabled_toolsets()
