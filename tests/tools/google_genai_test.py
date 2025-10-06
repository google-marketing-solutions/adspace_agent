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
"""Tests for the Google GenAI tools."""

from unittest import mock

import pytest

from adspace_agent.tools import google_genai

# pyright: reportAny=false


@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("google.genai.Client")
def test_get_info_about_youtube_video_success(mock_genai_client: mock.Mock):
    """Tests that get_info_about_youtube_video returns a success status."""
    mock_response = mock.Mock()
    mock_response.text = "Test response"
    mock_genai_client.return_value.models.generate_content.return_value = (
        mock_response
    )

    result = google_genai.get_info_about_youtube_video.func(
        "test_video_id", "test_prompt"
    )

    assert result["status"] == "SUCCESS"
    assert result["response"] == "Test response"


@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("google.genai.Client")
def test_get_info_about_youtube_video_error(mock_genai_client: mock.Mock):
    """Tests that get_info_about_youtube_video returns an error status."""
    mock_genai_client.return_value.models.generate_content.side_effect = (
        Exception("Test error")
    )

    result = google_genai.get_info_about_youtube_video.func(
        "test_video_id", "test_prompt"
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Test error"


@pytest.mark.asyncio
async def test_google_genai_toolset():
    """Tests that the GoogleGenAIToolset returns the correct tools."""
    toolset = google_genai.GoogleGenAIToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 1
    assert tools[0] is google_genai.get_info_about_youtube_video
