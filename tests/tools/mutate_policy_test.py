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
"""Tests for mutation policy, account allowlist, and confirmation gating."""

import types
import typing
from unittest import mock

from google.adk.tools.tool_confirmation import ToolConfirmation
import pytest

from adspace_agent.tools import mutate_policy
from adspace_agent.tools.untrusted_content import UNTRUSTED_TURN_INVOCATION_KEY


def _make_rest_tool(
    name: str,
    method: str,
    operation_id: str | None = None,
    *,
    wrapped: bool = False,
) -> mock.MagicMock:
    """Builds a mock RestApiTool or GoogleApiTool with endpoint metadata.

    Returns:
        Configured mock tool.
    """
    rest_tool = mock.MagicMock(spec=["name", "endpoint", "operation"])
    rest_tool.name = name
    rest_tool.endpoint = types.SimpleNamespace(method=method)
    rest_tool.operation = (
        types.SimpleNamespace(operationId=operation_id)
        if operation_id is not None
        else None
    )
    if not wrapped:
        return rest_tool
    outer = mock.MagicMock()
    outer.name = name
    outer._rest_api_tool = rest_tool  # ruff: ignore[private-member-access]
    return outer


def _make_plain_tool(name: str) -> mock.MagicMock:
    """Builds a mock non-REST tool without endpoint metadata.

    Returns:
        Configured mock tool.
    """
    tool = mock.MagicMock(spec=["name"])
    tool.name = name
    return tool


def _make_tool_context(
    *,
    invocation_id: str | None = "inv-1",
    state: dict[str, typing.Any] | None = None,
    tool_confirmation: ToolConfirmation | None = None,
) -> mock.MagicMock:
    """Builds a mock ToolContext for callback tests.

    Returns:
        Configured mock ToolContext.
    """
    ctx = mock.MagicMock()
    ctx.invocation_id = invocation_id
    ctx.state = {} if state is None else state
    ctx.tool_confirmation = tool_confirmation
    ctx.actions = types.SimpleNamespace(skip_summarization=False)
    return ctx


@pytest.mark.parametrize(
    "tool",
    [
        _make_plain_tool("traffic_campaigns_in_cm360_tool"),
        _make_rest_tool(
            "displayvideo_custom_bidding_algorithms_upload_script",
            "GET",
            "displayvideo.customBiddingAlgorithms.uploadScript",
            wrapped=True,
        ),
        _make_rest_tool(
            "displayvideo_custom_bidding_algorithms_upload_rules",
            "GET",
            "displayvideo.customBiddingAlgorithms.uploadRules",
            wrapped=True,
        ),
        _make_rest_tool(
            "displayvideo_advertisers_campaigns_create",
            "POST",
            "displayvideo.advertisers.campaigns.create",
            wrapped=True,
        ),
        _make_rest_tool(
            "dfareporting_campaigns_update",
            "PUT",
            "dfareporting.campaigns.update",
            wrapped=True,
        ),
        _make_rest_tool(
            "displayvideo_advertisers_line_items_patch",
            "PATCH",
            "displayvideo.advertisers.lineItems.patch",
            wrapped=True,
        ),
        _make_rest_tool(
            "doubleclickbidmanager_queries_delete",
            "DELETE",
            "doubleclickbidmanager.queries.delete",
            wrapped=True,
        ),
        _make_rest_tool(
            "custom_weird_method_tool",
            "CONNECT",
            "custom.weird.op",
        ),
        _make_plain_tool("displayvideo_advertisers_creatives_create"),
        _make_plain_tool("merchantapi_accounts_product_inputs_insert"),
        _make_plain_tool("dfareporting_ads_batchinsert"),
        _make_plain_tool("dfareporting_campaigns_patch"),
        _make_plain_tool("dfareporting_ads_update"),
        _make_plain_tool("dfareporting_conversions_batchupdate"),
        _make_plain_tool("displayvideo_advertisers_ad_assets_bulk_create"),
        _make_plain_tool("displayvideo_advertisers_line_items_bulk_update"),
        _make_plain_tool(
            "displayvideo_advertisers_line_items_bulk_edit_assigned_targe"
        ),
        _make_plain_tool("displayvideo_users_delete"),
        _make_plain_tool("some_remove_action"),
        _make_plain_tool("displayvideo_media_upload"),
        _make_plain_tool(
            "displayvideo_partners_edit_assigned_targeting_options"
        ),
        _make_plain_tool("displayvideo_advertisers_channels_sites_replace"),
        _make_plain_tool("displayvideo_advertisers_line_items_duplicate"),
        _make_plain_tool("dfareporting_dynamic_profiles_publish"),
        _make_plain_tool("dfareporting_dynamic_feeds_retransform"),
        _make_plain_tool("googleads_customers_campaigns_mutate"),
        _make_plain_tool("doubleclickbidmanager_queries_run"),
    ],
)
def test_is_mutating_tool_true(tool: mock.MagicMock) -> None:
    """Tests that mutating marketing and write tools return True."""
    assert mutate_policy.is_mutating_tool(tool) is True


@pytest.mark.parametrize(
    "tool",
    [
        _make_plain_tool("generate_image"),
        _make_plain_tool("generate_video"),
        _make_plain_tool("get_info_about_youtube_video"),
        _make_plain_tool("parse_sheet_tool"),
        _make_plain_tool("list_user_files"),
        _make_plain_tool("reload_skills"),
        _make_rest_tool(
            "displayvideo_advertisers_get",
            "GET",
            "displayvideo.advertisers.get",
            wrapped=True,
        ),
        _make_rest_tool("health_head", "HEAD"),
        _make_rest_tool("cors_options", "OPTIONS"),
        _make_rest_tool(
            "googleads_customers_google_ads_search",
            "POST",
            "googleads.customers.googleAds.search",
            wrapped=True,
        ),
        _make_rest_tool(
            "searchads360_customers_search_ads360_search",
            "POST",
            "searchads360.customers.searchAds360.search",
            wrapped=True,
        ),
        _make_rest_tool(
            "custom_colon_search",
            "POST",
            "api.items:search",
        ),
        _make_rest_tool(
            "dfareporting_report_data_query",
            "POST",
            "dfareporting.reportData.query",
            wrapped=True,
        ),
        _make_rest_tool(
            "custom_colon_query",
            "POST",
            "api.items:query",
        ),
        _make_rest_tool(
            "displayvideo_advertisers_reach_forecast_generate_reach_forec",
            "POST",
            "displayvideo.advertisers.reachForecast.generateReachForecast",
            wrapped=True,
        ),
        _make_rest_tool(
            "googleads_customers_generate_audience_definition",
            "POST",
            "googleads.customers.generateAudienceDefinition",
            wrapped=True,
        ),
        _make_rest_tool(
            "googleads_customers_generate_audience_composition_insights",
            "POST",
            "googleads.customers.generateAudienceCompositionInsights",
            wrapped=True,
        ),
        _make_rest_tool(
            "displayvideo_advertisers_audit",
            "POST",
            "displayvideo.advertisers.audit",
            wrapped=True,
        ),
        _make_plain_tool("list_skills"),
        _make_plain_tool("load_skill"),
        _make_plain_tool("get_current_date"),
        _make_plain_tool("data_analysis_agent"),
    ],
)
def test_is_mutating_tool_false(tool: mock.MagicMock) -> None:
    """Tests that read-only, query, and internal safe tools return False."""
    assert mutate_policy.is_mutating_tool(tool) is False


def test_extract_target_account_ids() -> None:
    """Tests extracting normalized IDs from nested args and resource paths."""
    args = {
        "advertiserId": "111",
        "customer_id": "123-456-7890",
        "account_id": 222,
        "profileId": "userProfiles/333",
        "partner_id": "partners/444",
        "merchant_id": "accounts/555",
        "login_customer_id": "999-888-7777",
        "ignored_bool": True,
        "customerId": True,
        "advertiser_id": "   ",
        "name": "accounts/666/productInputs/999",
        "parent": "customers/777-888-9999/campaigns/12",
        "resource_name": "partners/10/advertisers/20",
        "nested": [
            {"customerId": "321-654-0987"},
            ("ignored", {"parent": "accounts/"}),
        ],
    }
    assert mutate_policy.extract_target_account_ids(args) == {
        "111",
        "1234567890",
        "222",
        "333",
        "444",
        "555",
        "9998887777",
        "666",
        "7778889999",
        "10",
        "20",
        "3216540987",
    }


def test_callback_allows_readonly_tool() -> None:
    """Tests read-only tools pass through without gating or confirmation."""
    tool = _make_rest_tool("displayvideo_advertisers_get", "GET", wrapped=True)
    ctx = _make_tool_context()
    assert (
        mutate_policy.enforce_mutate_tool_policy_callback(tool, {}, ctx) is None
    )
    ctx.request_confirmation.assert_not_called()


def test_callback_account_allowlist_checks() -> None:
    """Tests account allowlist blocking and allowance behavior."""
    tool = _make_rest_tool(
        "displayvideo_advertisers_campaigns_create", "POST", wrapped=True
    )

    with mock.patch.dict(
        "os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": "123-456-7890,999"}
    ):
        ctx_missing = _make_tool_context()
        err_missing = mutate_policy.enforce_mutate_tool_policy_callback(
            tool, {"body": {"displayName": "Test"}}, ctx_missing
        )
        assert err_missing is not None
        assert "could not be determined" in err_missing["error"]
        ctx_missing.request_confirmation.assert_not_called()

        ctx_bad = _make_tool_context()
        err_bad = mutate_policy.enforce_mutate_tool_policy_callback(
            tool, {"advertiserId": "888"}, ctx_bad
        )
        assert err_bad is not None
        assert "888" in err_bad["error"]
        assert "ALLOWED_MUTATE_ACCOUNT_IDS" in err_bad["error"]
        ctx_bad.request_confirmation.assert_not_called()

        ctx_ok = _make_tool_context()
        err_confirm = mutate_policy.enforce_mutate_tool_policy_callback(
            tool, {"customerId": "1234567890"}, ctx_ok
        )
        assert err_confirm is not None
        assert "requires user confirmation" in err_confirm["error"]
        ctx_ok.request_confirmation.assert_called_once()

    with mock.patch.dict("os.environ", {"ALLOWED_MUTATE_ACCOUNT_IDS": "none"}):
        ctx_none = _make_tool_context()
        err_none = mutate_policy.enforce_mutate_tool_policy_callback(
            tool, {"advertiserId": "999"}, ctx_none
        )
        assert err_none is not None
        assert "ALLOWED_MUTATE_ACCOUNT_IDS" in err_none["error"]


def test_callback_blocks_same_turn_untrusted_taint() -> None:
    """Tests mutations are blocked in turns tainted by untrusted content."""
    tool = _make_rest_tool(
        "displayvideo_advertisers_campaigns_create", "POST", wrapped=True
    )
    tainted_ctx = _make_tool_context(
        invocation_id="inv-tainted",
        state={UNTRUSTED_TURN_INVOCATION_KEY: "inv-tainted"},
    )
    res = mutate_policy.enforce_mutate_tool_policy_callback(
        tool, {"advertiserId": "123"}, tainted_ctx
    )
    assert res is not None
    assert "same turn as untrusted external content" in res["error"]
    tainted_ctx.request_confirmation.assert_not_called()

    clean_turn_ctx = _make_tool_context(
        invocation_id="inv-new-turn",
        state={UNTRUSTED_TURN_INVOCATION_KEY: "inv-tainted"},
    )
    res_new = mutate_policy.enforce_mutate_tool_policy_callback(
        tool, {"advertiserId": "123"}, clean_turn_ctx
    )
    assert res_new is not None
    assert "requires user confirmation" in res_new["error"]
    clean_turn_ctx.request_confirmation.assert_called_once()


def test_callback_confirmation_states() -> None:
    """Tests pending, confirmed, and rejected ToolConfirmation states."""
    tool = _make_rest_tool(
        "dfareporting_campaigns_insert", "POST", wrapped=True
    )

    ctx_unconfirmed = _make_tool_context(tool_confirmation=None)
    pending_res = mutate_policy.enforce_mutate_tool_policy_callback(
        tool, {"profileId": "42"}, ctx_unconfirmed
    )
    assert pending_res is not None
    assert "requires user confirmation" in pending_res["error"]
    assert ctx_unconfirmed.actions.skip_summarization is True
    ctx_unconfirmed.request_confirmation.assert_called_once_with(
        hint="Approve executing mutating tool 'dfareporting_campaigns_insert'?",
        payload={
            "tool": "dfareporting_campaigns_insert",
            "args": {"profileId": "42"},
        },
    )

    ctx_confirmed = _make_tool_context(
        tool_confirmation=ToolConfirmation(confirmed=True)
    )
    assert (
        mutate_policy.enforce_mutate_tool_policy_callback(
            tool, {"profileId": "42"}, ctx_confirmed
        )
        is None
    )

    ctx_rejected = _make_tool_context(
        tool_confirmation=ToolConfirmation(confirmed=False)
    )
    rejected_res = mutate_policy.enforce_mutate_tool_policy_callback(
        tool, {"profileId": "42"}, ctx_rejected
    )
    assert rejected_res == {
        "error": (
            "Tool call 'dfareporting_campaigns_insert' was rejected by the "
            "user."
        )
    }
