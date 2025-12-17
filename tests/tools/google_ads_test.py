# Copyright 2025 Google LLC
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
"""A set of tests for the Google Ads tools."""

from unittest import mock

import pytest

from adspace_agent.tools import google_ads

# pyright: reportAny=false


@mock.patch("adspace_agent.tools.google_ads.client")
def test_list_accounts_success(mock_google_ads_client: mock.Mock):
    """Tests that list_accounts returns a successful response."""
    mock_customer_service = mock.Mock()
    mock_accessible_customers = mock.Mock()
    type(mock_accessible_customers).to_json = mock.Mock(return_value="[]")
    mock_customer_service.list_accessible_customers.return_value = (
        mock_accessible_customers
    )
    mock_google_ads_client.get_service.return_value = mock_customer_service

    response = google_ads.list_accounts.func()

    assert "error_details" not in response
    assert response["status"] == "SUCCESS"
    assert mock_google_ads_client.login_customer_id is None


@mock.patch("adspace_agent.tools.google_ads.client")
def test_list_accounts_error(mock_google_ads_client: mock.Mock):
    """Tests that list_accounts returns an error response."""
    mock_google_ads_client.get_service.side_effect = Exception("Test")

    response = google_ads.list_accounts.func()

    assert response["status"] == "ERROR"
    assert response["error_details"] == "Test"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_search_stream_success(mock_google_ads_client: mock.Mock):
    """Tests that search_stream returns a successful response."""
    mock_google_ads_service = mock.Mock()

    mock_row = mock.Mock()
    type(mock_row).to_json = mock.Mock(return_value='{"id": 1}')

    mock_batch = mock.Mock()
    mock_batch.results = [mock_row]

    mock_google_ads_service.search_stream.return_value = [mock_batch]
    mock_google_ads_client.get_service.return_value = mock_google_ads_service

    response = google_ads.search_stream.func(
        customer_id="123", query="query", login_customer_id="456"
    )

    assert "error_details" not in response
    assert response["status"] == "SUCCESS"
    assert response["data"] == ['{"id": 1}']
    assert mock_google_ads_client.login_customer_id == "456"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_search_stream_error(mock_google_ads_client: mock.Mock):
    """Tests that search_stream returns an error response."""
    mock_google_ads_client.get_service.side_effect = Exception("Test")

    response = google_ads.search_stream.func(
        customer_id="123",
        query="query",
    )

    assert response["status"] == "ERROR"
    assert mock_google_ads_client.login_customer_id is None


@mock.patch("adspace_agent.tools.google_ads.client")
def test_account_hierarchy_success_no_login_customer_id(
    mock_google_ads_client: mock.Mock,
):
    """Tests account_hierarchy success without login_customer_id."""
    mock_google_ads_service = mock.Mock()

    mock_customer_service = mock.Mock()

    # Mock list_accessible_customers
    mock_accessible_customers = mock.Mock()
    mock_accessible_customers.resource_names = ["customers/123"]
    mock_customer_service.list_accessible_customers.return_value = (
        mock_accessible_customers
    )

    # Mock parse_customer_path
    mock_google_ads_service.parse_customer_path.return_value = {
        "customer_id": "123"
    }

    # Mock search for hierarchy
    mock_root_row = mock.Mock()
    mock_root_row.customer_client.level = 0
    mock_root_row.customer_client.id = 123
    mock_root_row.customer_client.manager = False
    mock_root_row.customer_client.descriptive_name = "Root"
    mock_root_row.customer_client.currency_code = "USD"
    mock_root_row.customer_client.time_zone = "UTC"

    mock_google_ads_service.search.return_value = [mock_root_row]

    mock_google_ads_client.get_service.side_effect = [
        mock_google_ads_service,
        mock_customer_service,
    ]

    response = google_ads.account_hierarchy.func()

    assert "error_details" not in response
    assert response["status"] == "SUCCESS"
    assert len(response["data"]) == 1
    assert response["data"][0]["id"] == 123
    assert mock_google_ads_client.login_customer_id is None


@mock.patch("adspace_agent.tools.google_ads.client")
def test_account_hierarchy_success_with_children(
    mock_google_ads_client: mock.Mock,
):
    """Tests account_hierarchy success with children."""
    mock_google_ads_service = mock.Mock()

    mock_customer_service = mock.Mock()

    # Root account (manager)
    mock_root_client = mock.Mock()
    mock_root_client.level = 0
    mock_root_client.id = 1
    mock_root_client.manager = True
    mock_root_client.descriptive_name = "Root"
    mock_root_client.currency_code = "USD"
    mock_root_client.time_zone = "UTC"
    mock_root_row = mock.Mock()
    mock_root_row.customer_client = mock_root_client

    # Child account
    mock_child_client = mock.Mock()
    mock_child_client.level = 1
    mock_child_client.id = 2
    mock_child_client.manager = False
    mock_child_client.descriptive_name = "Child"
    mock_child_client.currency_code = "USD"
    mock_child_client.time_zone = "UTC"
    mock_child_row = mock.Mock()
    mock_child_row.customer_client = mock_child_client

    # Second manager (level 1)
    mock_mgr2_client_level1 = mock.Mock()
    mock_mgr2_client_level1.level = 1
    mock_mgr2_client_level1.id = 3
    mock_mgr2_client_level1.manager = True
    mock_mgr2_client_level1.descriptive_name = "Mgr2"
    mock_mgr2_client_level1.currency_code = "USD"
    mock_mgr2_client_level1.time_zone = "UTC"
    mock_mgr2_row_level1 = mock.Mock()
    mock_mgr2_row_level1.customer_client = mock_mgr2_client_level1

    # Second manager as root (level 0)
    mock_mgr2_client_level0 = mock.Mock()
    mock_mgr2_client_level0.level = 0
    mock_mgr2_client_level0.id = 3
    mock_mgr2_client_level0.manager = True
    mock_mgr2_client_level0.descriptive_name = "Mgr2"
    mock_mgr2_client_level0.currency_code = "USD"
    mock_mgr2_client_level0.time_zone = "UTC"
    mock_mgr2_row_level0 = mock.Mock()
    mock_mgr2_row_level0.customer_client = mock_mgr2_client_level0

    # Search results for root (1)
    mock_google_ads_service.search.side_effect = [
        [
            mock_root_row,
            mock_child_row,
            mock_mgr2_row_level1,
        ],  # Results for root
        [mock_mgr2_row_level0],  # Results for Mgr2 (itself, level 0)
    ]

    mock_google_ads_client.get_service.side_effect = [
        mock_google_ads_service,
        mock_customer_service,
    ]

    response = google_ads.account_hierarchy.func(login_customer_id="1")

    assert response["status"] == "SUCCESS"
    assert len(response["data"]) == 1
    root = response["data"][0]
    assert root["id"] == 1
    assert len(root["children"]) == 2
    assert root["children"][0]["id"] == 2
    assert root["children"][1]["id"] == 3
    assert mock_google_ads_client.login_customer_id == "1"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_account_hierarchy_error(mock_google_ads_client: mock.Mock):
    """Tests that account_hierarchy returns an error response."""
    mock_google_ads_client.get_service.side_effect = Exception("Test")

    response = google_ads.account_hierarchy.func()

    assert response["status"] == "ERROR"


@pytest.mark.asyncio
async def test_google_ads_toolset():
    """Tests that the GoogleAdsToolset returns the correct tools."""
    toolset = google_ads.GoogleAdsToolset()
    tools = await toolset.get_tools()

    assert len(tools) == 3

    assert tools[0] is google_ads.list_accounts
    assert tools[1] is google_ads.search_stream
    assert tools[2] is google_ads.account_hierarchy
