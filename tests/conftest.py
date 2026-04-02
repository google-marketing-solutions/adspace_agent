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

import pytest

# Mock classes to prevent actual initialization and refresh


class MockGenAIClient:
    """A dummy GenAI client for test collection."""

    aio: mock.Mock
    models: mock.Mock
    operations: mock.Mock

    def __init__(self, *args: object, **kwargs: object):  # noqa: ARG002 # pylint: disable=unused-argument
        """Initializes the mock client."""
        self.aio = mock.Mock()
        self.models = mock.Mock()
        self.operations = mock.Mock()


# Set up dummy environment variables for test collection.
# These must be set before any modules that initialize clients are imported.
DUMMY_ENV = {
    "GOOGLE_CLOUD_PROJECT": "dummy-project",
    "GOOGLE_CLOUD_LOCATION": "dummy-location",
    "GOOGLE_ADS_DEVELOPER_TOKEN": "dummy-token",
    "CLIENT_ID": "dummy-client-id",
    "CLIENT_SECRET": "dummy-client-secret",
}

_mock_env = mock.patch.dict(os.environ, DUMMY_ENV)
_mock_env.start()  # pyright: ignore[reportAny]


_mock_genai_client = mock.patch("google.genai.Client", MockGenAIClient)
_mock_genai_client.start()  # pyright: ignore[reportUnusedCallResult]

_mock_google_auth = mock.patch(
    "google.auth.default", return_value=(mock.Mock(), "dummy-project")
)
_mock_google_auth.start()  # pyright: ignore[reportUnusedCallResult]

# Mock discovery resource for GoogleApiToOpenApiConverter


def pytest_unconfigure(config: pytest.Config):  # noqa: ARG001 # pyright: ignore[reportUnusedParameter] # pylint: disable=unused-argument
    """Clean up mocks after tests are done."""
    _mock_google_auth.stop()
    _mock_genai_client.stop()
    _mock_env.stop()  # pyright: ignore[reportAny]
