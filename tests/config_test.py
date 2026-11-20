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

import asyncio
import json
import pathlib
from unittest.mock import patch

from google.adk.tools.google_api_tool.googleapi_to_openapi_converter import (
    GoogleApiToOpenApiConverter,
)
from google.adk.tools.openapi_tool import OpenAPIToolset
import googleapiclient.discovery_cache
import pytest

import adspace_agent.config as config_module
from tests import conftest


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


def test_resolve_tool_filter_variants():
    """Test resolve_tool_filter with unset, empty, wildcard, none, and list."""
    default = ["tool_x", "tool_y"]
    with patch.dict("os.environ", {}, clear=False):
        config_module.os.environ.pop("CUSTOM_FILTER", None)
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) == (
            default
        )

    with patch.dict("os.environ", {"CUSTOM_FILTER": " , "}):
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) == (
            default
        )

    with patch.dict("os.environ", {"CUSTOM_FILTER": "*"}):
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) is (
            None
        )

    with patch.dict("os.environ", {"CUSTOM_FILTER": "ALL"}):
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) is (
            None
        )

    with patch.dict("os.environ", {"CUSTOM_FILTER": "none"}):
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) == []

    with patch.dict("os.environ", {"CUSTOM_FILTER": " a , b "}):
        assert config_module.resolve_tool_filter("CUSTOM_FILTER", default) == [
            "a",
            "b",
        ]


def test_get_toolset_filters():
    """Test get_toolset_filters defaults and per-toolset env overrides."""
    with patch.dict(
        "os.environ",
        {
            "DISPLAY_VIDEO_360_TOOL_FILTER": "dv_custom",
            "BID_MANAGER_TOOL_FILTER": "none",
        },
        clear=False,
    ):
        for key in (
            "CAMPAIGN_MANAGER_360_TOOL_FILTER",
            "GOOGLE_ADS_TOOL_FILTER",
            "MERCHANT_CENTER_INVENTORIES_TOOL_FILTER",
            "MERCHANT_CENTER_PRODUCTS_TOOL_FILTER",
            "MERCHANT_CENTER_REPORTS_TOOL_FILTER",
            "SEARCH_ADS_360_TOOL_FILTER",
        ):
            config_module.os.environ.pop(key, None)
        filters = config_module.get_toolset_filters()
        assert filters["bid_manager"] == []
        assert filters["display_video_360"] == ["dv_custom"]
        assert (
            filters["campaign_manager_360"]
            == config_module.CAMPAIGN_MANAGER_360_TOOL_FILTER
        )
        assert (
            filters["google_ads"]
            == config_module.DEFAULT_GOOGLE_ADS_TOOL_FILTER
        )
        assert (
            filters["merchant_center_inventories"]
            == config_module.MERCHANT_CENTER_INVENTORIES_TOOL_FILTER
        )
        assert (
            filters["merchant_center_products"]
            == config_module.MERCHANT_CENTER_PRODUCTS_TOOL_FILTER
        )
        assert (
            filters["merchant_center_reports"]
            == config_module.MERCHANT_CENTER_REPORTS_TOOL_FILTER
        )
        assert (
            filters["search_ads_360"]
            == config_module.SEARCH_ADS_360_TOOL_FILTER
        )


def test_normalize_account_id():
    """Test normalize_account_id strips prefixes, whitespace, and hyphens."""
    assert config_module.normalize_account_id(" 123-456-7890 ") == "1234567890"
    assert (
        config_module.normalize_account_id("customers/123-456-7890")
        == "1234567890"
    )
    assert config_module.normalize_account_id("advertisers/42") == "42"
    assert config_module.normalize_account_id("accounts/99") == "99"
    assert config_module.normalize_account_id("partners/7") == "7"
    assert config_module.normalize_account_id("userProfiles/55") == "55"
    assert not config_module.normalize_account_id("  ")


def test_get_allowed_mutate_account_ids():
    """Test get_allowed_mutate_account_ids across all env variants."""
    with patch.dict("os.environ", {}, clear=False):
        config_module.os.environ.pop("ALLOWED_MUTATE_ACCOUNT_IDS", None)
        assert config_module.get_allowed_mutate_account_ids() is None

    with patch.dict("os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": " , "}):
        assert config_module.get_allowed_mutate_account_ids() is None

    with patch.dict("os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": "*"}):
        assert config_module.get_allowed_mutate_account_ids() is None

    with patch.dict("os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": "ALL"}):
        assert config_module.get_allowed_mutate_account_ids() is None

    with patch.dict("os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": "none"}):
        assert config_module.get_allowed_mutate_account_ids() == set()

    with patch.dict(
        "os.environ",
        {
            "ALLOWED_MUTATE_ACCOUNT_IDS": (
                "customers/123-456-7890, advertisers/99, customers/"
            )
        },
    ):
        assert config_module.get_allowed_mutate_account_ids() == {
            "1234567890",
            "99",
        }


def test_marketing_tool_filters_are_readonly_in_discovery_specs():
    """Test default marketing tool filters exclude all write/mutate ops."""
    dc_dir = (
        pathlib.Path(googleapiclient.discovery_cache.__file__).parent
        / "documents"
    )
    readonly_post_allowlist = {
        "dfareporting_dimension_values_query",
        "dfareporting_report_data_query",
        "dfareporting_reports_compatible_fields_query",
        "displayvideo_targeting_types_targeting_options_search",
        "merchantapi_accounts_reports_search",
        "searchads360_customers_search_ads360_search",
        "searchads360_search_ads360_fields_search",
    }
    specs_and_filters = [
        (
            "doubleclickbidmanager",
            "v2",
            "doubleclickbidmanager.v2.json",
            config_module.BID_MANAGER_TOOL_FILTER,
        ),
        (
            "dfareporting",
            "v5",
            "dfareporting.v5.json",
            config_module.CAMPAIGN_MANAGER_360_TOOL_FILTER,
        ),
        (
            "displayvideo",
            "v4",
            "displayvideo.v4.json",
            config_module.DISPLAY_VIDEO_360_TOOL_FILTER,
        ),
        (
            "merchantapi",
            "inventories_v1",
            "merchantapi.inventories_v1.json",
            config_module.MERCHANT_CENTER_INVENTORIES_TOOL_FILTER,
        ),
        (
            "merchantapi",
            "products_v1",
            "merchantapi.products_v1.json",
            config_module.MERCHANT_CENTER_PRODUCTS_TOOL_FILTER,
        ),
        (
            "merchantapi",
            "reports_v1",
            "merchantapi.reports_v1.json",
            config_module.MERCHANT_CENTER_REPORTS_TOOL_FILTER,
        ),
        (
            "searchads360",
            "v0",
            "searchads360.v0.json",
            config_module.SEARCH_ADS_360_TOOL_FILTER,
        ),
    ]

    mock_conv = (
        conftest._mock_openapi_converter  # ruff: ignore[private-member-access]
    )
    with patch.object(
        GoogleApiToOpenApiConverter,
        "convert",
        mock_conv.temp_original,
    ):
        for api_name, api_ver, filename, tool_filter in specs_and_filters:
            doc = json.loads((dc_dir / filename).read_text(encoding="utf-8"))
            conv = GoogleApiToOpenApiConverter(api_name, api_ver)
            conv._google_api_spec = doc  # ruff: ignore[private-member-access]
            openapi_spec = conv.convert()
            tools = asyncio.run(
                OpenAPIToolset(spec_dict=openapi_spec).get_tools()
            )
            tools_by_name = {tool.name: tool for tool in tools}

            assert len(tool_filter) == len(set(tool_filter))
            for allowed_name in tool_filter:
                assert allowed_name in tools_by_name
                method = tools_by_name[allowed_name].endpoint.method.upper()
                assert (
                    method == "GET" or allowed_name in readonly_post_allowlist
                ), f"Non-readonly tool {allowed_name} ({method}) in {api_name}"
                assert "upload_rules" not in allowed_name
                assert "upload_script" not in allowed_name

            for tool in tools:
                method = tool.endpoint.method.upper()
                if method != "GET" and tool.name not in readonly_post_allowlist:
                    assert tool.name not in tool_filter

    poc_dv360_mutates = {
        "displayvideo_advertisers_campaigns_create",
        "displayvideo_advertisers_campaigns_patch",
        "displayvideo_advertisers_insertion_orders_create",
        "displayvideo_advertisers_insertion_orders_patch",
        "displayvideo_advertisers_line_items_create",
        "displayvideo_advertisers_line_items_patch",
        "displayvideo_advertisers_line_items_bulk_update",
        "displayvideo_advertisers_line_items_bulk_edit_assigned_targe",
        "displayvideo_advertisers_line_items_duplicate",
        "displayvideo_advertisers_creatives_create",
        "displayvideo_advertisers_creatives_patch",
        "displayvideo_advertisers_ad_assets_create",
        "displayvideo_advertisers_edit_assigned_targeting_options",
        "displayvideo_custom_bidding_algorithms_upload_rules",
        "displayvideo_custom_bidding_algorithms_upload_script",
        "displayvideo_advertisers_line_items_generate_default",
    }
    assert not (
        poc_dv360_mutates & set(config_module.DISPLAY_VIDEO_360_TOOL_FILTER)
    )
