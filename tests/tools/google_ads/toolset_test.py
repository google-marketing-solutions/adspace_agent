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

from unittest import mock

from google.adk.auth.auth_schemes import OpenIdConnectWithConfig
import pytest

from adspace_agent.tools.google_ads.toolset import GOOGLE_ADS_API_VERSION
from adspace_agent.tools.google_ads.toolset import GoogleAdsToolset


@pytest.fixture
def mock_discovery_converter():
    with mock.patch(
        "adspace_agent.tools.google_ads.toolset.DiscoveryConverter"
    ) as mock_conv:
        mock_instance = mock_conv.return_value
        mock_instance.convert.return_value = {
            "openapi": "3.0.0",
            "info": {"title": "Test", "version": "v1"},
            "paths": {
                "/v1/test": {
                    "get": {
                        "operationId": "test_operation",
                        "responses": {"200": {"description": "OK"}},
                    }
                }
            },
        }
        yield mock_conv


@pytest.fixture
def mock_openapi_toolset():
    with mock.patch(
        "adspace_agent.tools.google_ads.toolset.OpenAPIToolset"
    ) as mock_toolset:
        instance = mock_toolset.return_value
        instance.get_tools = mock.AsyncMock(return_value=["tool1", "tool2"])
        instance.get_auth_config.return_value = "mock_auth_config"
        instance.close = mock.AsyncMock()
        yield mock_toolset


@pytest.mark.asyncio
async def test_google_ads_toolset_init(
    mock_discovery_converter,
    mock_openapi_toolset,
):
    toolset = GoogleAdsToolset(
        developer_token="dev_token",  # noqa: S106
        client_id="client_id",
        client_secret="client_secret",  # noqa: S106
        login_customer_id="1234567890",
        tool_filter=["test_operation"],
    )

    assert toolset.developer_token == "dev_token"  # noqa: S105
    assert toolset.client_id == "client_id"
    assert toolset.client_secret == "client_secret"  # noqa: S105
    assert toolset.login_customer_id == "1234567890"

    mock_discovery_converter.assert_called_once_with(
        f"https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}"
    )
    mock_openapi_toolset.assert_called_once()

    # Extract the header provider to test it
    kwargs = mock_openapi_toolset.call_args.kwargs
    header_provider = kwargs["header_provider"]
    auth_scheme = kwargs["auth_scheme"]

    assert isinstance(auth_scheme, OpenIdConnectWithConfig)
    assert (
        "https://accounts.google.com/o/oauth2/v2/auth"
        in auth_scheme.authorization_endpoint
    )

    headers = header_provider(context=None)
    assert headers["developer-token"] == "dev_token"
    assert headers["login-customer-id"] == "1234567890"

    # Test get_tools
    tools = await toolset.get_tools()
    expected_tools_length = 2
    assert len(tools) == expected_tools_length
    assert tools[0] == "tool1"
    assert tools[1] == "tool2"

    # Test get_auth_config
    auth_config = toolset.get_auth_config()
    assert auth_config == "mock_auth_config"

    # Test close
    await toolset.close()
    mock_openapi_toolset.return_value.close.assert_called_once()


@pytest.mark.asyncio
async def test_header_provider_no_login_customer_id(
    mock_discovery_converter,  # noqa: ARG001
    mock_openapi_toolset,
):
    _ = GoogleAdsToolset(
        developer_token="dev_token",  # noqa: S106
        client_id="client_id",
        client_secret="client_secret",  # noqa: S106
    )

    kwargs = mock_openapi_toolset.call_args.kwargs
    header_provider = kwargs["header_provider"]

    headers = header_provider(context=None)
    assert headers["developer-token"] == "dev_token"
    assert "login-customer-id" not in headers


@pytest.mark.asyncio
async def test_google_ads_toolset_pruning(
    mock_discovery_converter,
    mock_openapi_toolset,
):
    mock_instance = mock_discovery_converter.return_value
    mock_instance.convert.return_value = {
        "openapi": "3.0.0",
        "info": {"title": "Test", "version": "v1"},
        "paths": {
            "/v1/test1": {
                "get": {
                    "operationId": "test_operation1",
                    "responses": {"200": {"description": "OK"}},
                }
            },
            "/v1/test2": {
                "get": {
                    "operationId": "test_operation2",
                    "responses": {"200": {"description": "OK"}},
                }
            },
        },
    }

    _ = GoogleAdsToolset(
        developer_token="dev_token",  # noqa: S106
        client_id="client_id",
        client_secret="client_secret",  # noqa: S106
        tool_filter=["test_operation1"],
    )

    kwargs = mock_openapi_toolset.call_args.kwargs
    spec_dict = kwargs["spec_dict"]

    assert "/v1/test1" in spec_dict["paths"]
    assert "/v1/test2" not in spec_dict["paths"]


@pytest.mark.asyncio
async def test_google_ads_toolset_pruning_callable(
    mock_discovery_converter,
    mock_openapi_toolset,
):
    mock_instance = mock_discovery_converter.return_value
    mock_instance.convert.return_value = {
        "openapi": "3.0.0",
        "info": {"title": "Test", "version": "v1"},
        "paths": {
            "/v1/test1": {
                "get": {
                    "operationId": "test_operation1",
                    "responses": {"200": {"description": "OK"}},
                }
            },
            "/v1/test2": {
                "get": {
                    "operationId": "test_operation2",
                    "responses": {"200": {"description": "OK"}},
                }
            },
        },
    }

    def my_filter(name: str) -> bool:
        return name == "test_operation2"

    _ = GoogleAdsToolset(
        developer_token="dev_token",  # noqa: S106
        client_id="client_id",
        client_secret="client_secret",  # noqa: S106
        tool_filter=my_filter,  # type: ignore # noqa: PGH003
    )

    kwargs = mock_openapi_toolset.call_args.kwargs
    spec_dict = kwargs["spec_dict"]

    assert "/v1/test1" not in spec_dict["paths"]
    assert "/v1/test2" in spec_dict["paths"]
