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

    assert response["status"] == "SUCCESS"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_list_accounts_error(mock_google_ads_client: mock.Mock):
    """Tests that list_accounts returns an error response."""
    mock_google_ads_client.get_service.side_effect = Exception("Test")

    response = google_ads.list_accounts.func()

    assert response["status"] == "ERROR"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_search_stream_success(mock_google_ads_client: mock.Mock):
    """Tests that search_stream returns a successful response."""
    mock_google_ads_service = mock.Mock()
    mock_google_ads_service.search_stream.return_value = []
    mock_google_ads_client.get_service.return_value = mock_google_ads_service

    response = google_ads.search_stream.func(
        customer_id="123",
        query="query",
    )

    assert response["status"] == "SUCCESS"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_search_stream_error(mock_google_ads_client: mock.Mock):
    """Tests that search_stream returns an error response."""
    mock_google_ads_client.get_service.side_effect = Exception("Test")

    response = google_ads.search_stream.func(
        customer_id="123",
        query="query",
    )

    assert response["status"], "ERROR"


@mock.patch("adspace_agent.tools.google_ads.client")
def test_account_hierarchy_success(mock_google_ads_client: mock.Mock):
    """Tests that account_hierarchy returns a successful response."""
    mock_google_ads_service = mock.Mock()
    mock_google_ads_service.search.return_value = []
    mock_customer_service = mock.Mock()
    mock_accessible_customers = mock.Mock()
    mock_accessible_customers.resource_names = []
    mock_customer_service.list_accessible_customers.return_value = (
        mock_accessible_customers
    )
    mock_google_ads_client.get_service.side_effect = [
        mock_google_ads_service,
        mock_customer_service,
    ]

    response = google_ads.account_hierarchy.func()

    assert response["status"] == "SUCCESS"


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
