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
"""Global pytest fixtures and configuration."""

import os
from unittest import mock

from google.ads.googleads.v21.services.services.google_ads_service import (
    GoogleAdsServiceClient,
)
import pytest


# Mock classes to prevent actual initialization and refresh
class MockGoogleAdsClient:
    """A dummy client for test collection."""

    login_customer_id: str | None

    def __init__(self, *args: object, **kwargs: object):  # pylint: disable=line-too-long,unused-argument
        self.login_customer_id = None

    @classmethod
    def load_from_env(cls):
        return cls()

    def get_service(self, name: str, version: str | None = None):  # pyright: ignore[reportUnusedParameter] # pylint: disable=line-too-long,unused-argument
        return mock.Mock(spec=GoogleAdsServiceClient)


class MockGenAIClient:
    """A dummy GenAI client for test collection."""

    aio: mock.Mock
    models: mock.Mock
    operations: mock.Mock

    def __init__(self, *args: object, **kwargs: object):  # pylint: disable=unused-argument
        self.aio = mock.Mock()
        self.models = mock.Mock()
        self.operations = mock.Mock()


# Set up dummy environment variables for test collection.
# These must be set before any modules that initialize clients are imported.
DUMMY_ENV = {
    "GOOGLE_CLOUD_PROJECT": "dummy-project",
    "GOOGLE_CLOUD_LOCATION": "dummy-location",
    "GOOGLE_ADS_DEVELOPER_TOKEN": "dummy-token",
    "GOOGLE_ADS_REFRESH_TOKEN": "dummy-refresh-token",
    "GOOGLE_ADS_CLIENT_ID": "dummy-client-id",
    "GOOGLE_ADS_CLIENT_SECRET": "dummy-client-secret",
    "GOOGLE_ADS_USE_PROTO_PLUS": "TRUE",
    # Disable automated refresh during tests
    "GOOGLE_ADS_CREDENTIALS_REFRESH_OFFSET": "3600",
    "CLIENT_ID": "dummy-client-id",
    "CLIENT_SECRET": "dummy-client-secret",
    "GOOGLE_API_KEY": "dummy-api-key",
}

_mock_env = mock.patch.dict(os.environ, DUMMY_ENV)
_mock_env.start()  # pyright: ignore[reportAny]

_mock_client = mock.patch(
    "google.ads.googleads.client.GoogleAdsClient", MockGoogleAdsClient
)
_mock_client.start()  # pyright: ignore[reportUnusedCallResult]

_mock_genai_client = mock.patch("google.genai.Client", MockGenAIClient)
_mock_genai_client.start()  # pyright: ignore[reportUnusedCallResult]

_mock_google_auth = mock.patch(
    "google.auth.default", return_value=(mock.Mock(), "dummy-project")
)
_mock_google_auth.start()  # pyright: ignore[reportUnusedCallResult]

# Mock discovery resource for GoogleApiToOpenApiConverter
_mock_discovery_resource = mock.Mock()
_mock_discovery_resource._rootDesc = {  # pylint: disable=protected-access
    "title": "Mock API",
    "description": "Mock Description",
    "version": "v1",
    "rootUrl": "https://example.com/",
    "servicePath": "api/v1",
    "resources": {},
    "auth": {
        "oauth2": {
            "scopes": {
                "https://www.googleapis.com/auth/cloud-platform": {
                    "description": "Mock scope"
                }
            }
        }
    },
    "schemas": {},
}
_mock_discovery_build = mock.patch(
    "googleapiclient.discovery.build", return_value=_mock_discovery_resource
)
_mock_discovery_build.start()  # pyright: ignore[reportUnusedCallResult]


def pytest_unconfigure(config: pytest.Config):  # pyright: ignore[reportUnusedParameter] # pylint: disable=unused-argument
    """Clean up mocks after tests are done."""
    _mock_discovery_build.stop()
    _mock_google_auth.stop()
    _mock_genai_client.stop()
    _mock_client.stop()
    _mock_env.stop()  # pyright: ignore[reportAny]
