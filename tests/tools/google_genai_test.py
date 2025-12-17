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

import importlib
from unittest import mock

import pytest

from adspace_agent.tools import google_genai

# pyright: reportAny=false


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_get_info_about_youtube_video_success(
    mock_genai_client: mock.Mock,
):
    """Tests that get_info_about_youtube_video returns a success status."""
    mock_response = mock.Mock()
    mock_response.text = "Test response"
    mock_genai_client.aio.models.generate_content = mock.AsyncMock(
        return_value=mock_response
    )

    result = await google_genai.get_info_about_youtube_video.func(
        "test_video_id", "test_prompt"
    )

    assert "error_details" not in result
    assert result["status"] == "SUCCESS"
    assert result["response"] == "Test response"

    # Verify call arguments
    mock_genai_client.aio.models.generate_content.assert_called_once()
    _, kwargs = mock_genai_client.aio.models.generate_content.call_args
    assert kwargs["model"] == google_genai.MODEL
    contents = kwargs["contents"][0]
    assert (
        contents.parts[0].file_data.file_uri
        == "https://www.youtube.com/watch?v=test_video_id"
    )
    assert contents.parts[1].text == "test_prompt"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_get_info_about_youtube_video_error(mock_genai_client: mock.Mock):
    """Tests that get_info_about_youtube_video returns an error status."""
    mock_genai_client.aio.models.generate_content = mock.AsyncMock(
        side_effect=Exception("Test error")
    )

    result = await google_genai.get_info_about_youtube_video.func(
        "test_video_id", "test_prompt"
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Test error"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_success(mock_genai_client: mock.Mock):
    """Tests that generate_video returns a success status."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response.generated_videos = [mock.Mock()]
    mock_operation.response.generated_videos[
        0
    ].video.video_bytes = b"test_video"
    mock_operation.response.generated_videos[0].video.mime_type = "video/mp4"
    mock_genai_client.models.generate_videos = mock.Mock(
        return_value=mock_operation
    )

    mock_tool_context = mock.AsyncMock()
    mock_tool_context.save_artifact.return_value = "v1"

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert "error_details" not in result
    assert result["status"] == "SUCCESS"
    assert result["message"] == "Generated video."

    # Verify call arguments
    mock_genai_client.models.generate_videos.assert_called_once()
    _, kwargs = mock_genai_client.models.generate_videos.call_args
    assert kwargs["model"] == "veo-3.1-fast-generate-preview"
    assert kwargs["source"].prompt == "test_prompt"

    mock_tool_context.save_artifact.assert_called_once()
    _, kwargs = mock_tool_context.save_artifact.call_args
    assert kwargs["filename"].startswith("generated_video_")
    artifact = kwargs["artifact"]
    assert artifact.inline_data.data == b"test_video"
    assert artifact.inline_data.mime_type == "video/mp4"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
@mock.patch("asyncio.sleep", return_value=None)
async def test_generate_video_polling(
    mock_sleep: mock.Mock, mock_genai_client: mock.Mock
):
    """Tests that generate_video polls until the operation is done."""
    mock_op_pending = mock.Mock()
    mock_op_pending.done = False

    mock_op_done = mock.Mock()
    mock_op_done.done = True
    mock_op_done.response.generated_videos = [mock.Mock()]
    mock_op_done.response.generated_videos[0].video.video_bytes = b"test_video"
    mock_op_done.response.generated_videos[0].video.mime_type = "video/mp4"

    mock_genai_client.models.generate_videos.return_value = mock_op_pending
    mock_genai_client.operations.get.return_value = mock_op_done

    mock_tool_context = mock.AsyncMock()
    mock_tool_context.save_artifact.return_value = "v1"

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "SUCCESS"
    assert mock_genai_client.operations.get.called
    assert mock_sleep.called


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_no_response(mock_genai_client: mock.Mock):
    """Tests that generate_video handles no response."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response = None
    mock_genai_client.models.generate_videos = mock.Mock(
        return_value=mock_operation
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Video generation failed: No response."


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_no_generated_videos(mock_genai_client: mock.Mock):
    """Tests that generate_video handles no generated videos."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response.generated_videos = []
    mock_genai_client.models.generate_videos = mock.Mock(
        return_value=mock_operation
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Video generation failed: No videos generated."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_missing_video_data(mock_genai_client: mock.Mock):
    """Tests that generate_video handles missing video data."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response.generated_videos = [mock.Mock()]
    mock_operation.response.generated_videos[0].video = None
    mock_genai_client.models.generate_videos = mock.Mock(
        return_value=mock_operation
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Video generation failed: Missing video data."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_empty_video_content(mock_genai_client: mock.Mock):
    """Tests that generate_video handles empty video content."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response.generated_videos = [mock.Mock()]
    mock_operation.response.generated_videos[0].video.video_bytes = None
    mock_operation.response.generated_videos[0].video.mime_type = None
    mock_genai_client.models.generate_videos = mock.Mock(
        return_value=mock_operation
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Video generation failed: Empty video content."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_exception(mock_genai_client: mock.Mock):
    """Tests that generate_video handles exceptions."""
    mock_genai_client.models.generate_videos = mock.Mock(
        side_effect=Exception("Test error")
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Test error"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_operations_get_exception(
    mock_genai_client: mock.Mock,
):
    """Tests that generate_video handles operations.get exceptions."""
    mock_op_pending = mock.Mock()
    mock_op_pending.done = False

    mock_genai_client.models.generate_videos.return_value = mock_op_pending
    mock_genai_client.operations.get.side_effect = Exception(
        "Operation get error"
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Operation get error"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_video_save_artifact_exception(
    mock_genai_client: mock.Mock,
):
    """Tests that generate_video handles save_artifact exceptions."""
    mock_operation = mock.Mock()
    mock_operation.done = True
    mock_operation.response.generated_videos = [mock.Mock()]
    mock_operation.response.generated_videos[
        0
    ].video.video_bytes = b"test_video"
    mock_operation.response.generated_videos[0].video.mime_type = "video/mp4"

    mock_genai_client.models.generate_videos.return_value = mock_operation

    mock_tool_context = mock.AsyncMock()
    mock_tool_context.save_artifact.side_effect = Exception(
        "Save artifact error"
    )

    result = await google_genai.generate_video.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Save artifact error"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_success(mock_genai_client: mock.Mock):
    """Tests that generate_image returns a success status."""
    mock_response = mock.Mock()
    mock_response.generated_images = [mock.Mock()]
    mock_response.generated_images[0].image.image_bytes = b"test_image"
    mock_response.generated_images[0].image.mime_type = "image/png"
    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        return_value=mock_response
    )

    mock_tool_context = mock.AsyncMock()
    mock_tool_context.save_artifact.return_value = "v1"

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert "error_details" not in result
    assert result["status"] == "SUCCESS"
    assert result["message"] == "Generated image."

    # Verify call arguments
    mock_genai_client.aio.models.generate_images.assert_called_once()
    _, kwargs = mock_genai_client.aio.models.generate_images.call_args
    assert kwargs["model"] == "imagen-4.0-generate-001"
    assert kwargs["prompt"] == "test_prompt"

    mock_tool_context.save_artifact.assert_called_once()
    _, kwargs = mock_tool_context.save_artifact.call_args
    assert kwargs["filename"].startswith("generated_image_")
    artifact = kwargs["artifact"]
    assert artifact.inline_data.data == b"test_image"
    assert artifact.inline_data.mime_type == "image/png"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_no_generated_images(mock_genai_client: mock.Mock):
    """Tests that generate_image handles no generated images."""
    mock_response = mock.Mock()
    mock_response.generated_images = []
    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        return_value=mock_response
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Image generation failed: No images generated."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_missing_image_data(mock_genai_client: mock.Mock):
    """Tests that generate_image handles missing image data."""
    mock_response = mock.Mock()
    mock_response.generated_images = [mock.Mock()]
    mock_response.generated_images[0].image = None
    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        return_value=mock_response
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Image generation failed: Missing image data."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_empty_image_content(mock_genai_client: mock.Mock):
    """Tests that generate_image handles empty image content."""
    mock_response = mock.Mock()
    mock_response.generated_images = [mock.Mock()]
    mock_response.generated_images[0].image.image_bytes = None
    mock_response.generated_images[0].image.mime_type = None
    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        return_value=mock_response
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert (
        result["error_details"]
        == "Image generation failed: Empty image content."
    )


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_exception(mock_genai_client: mock.Mock):
    """Tests that generate_image handles exceptions."""
    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        side_effect=Exception("Test error")
    )

    mock_tool_context = mock.AsyncMock()

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Test error"


@pytest.mark.asyncio
@mock.patch.dict(
    "os.environ",
    {
        "GOOGLE_CLOUD_PROJECT": "test-project",
        "GOOGLE_CLOUD_LOCATION": "test-location",
    },
)
@mock.patch("adspace_agent.tools.google_genai.genai_client")
async def test_generate_image_save_artifact_exception(
    mock_genai_client: mock.Mock,
):
    """Tests that generate_image handles save_artifact exceptions."""
    mock_response = mock.Mock()
    mock_response.generated_images = [mock.Mock()]
    mock_response.generated_images[0].image.image_bytes = b"test_image"
    mock_response.generated_images[0].image.mime_type = "image/png"

    mock_genai_client.aio.models.generate_images = mock.AsyncMock(
        return_value=mock_response
    )

    mock_tool_context = mock.AsyncMock()
    mock_tool_context.save_artifact.side_effect = Exception(
        "Save artifact error"
    )

    result = await google_genai.generate_image.func(
        "test_prompt", mock_tool_context
    )

    assert result["status"] == "ERROR"
    assert result["error_details"] == "Save artifact error"


@pytest.mark.asyncio
async def test_google_genai_toolset():
    """Tests that the GoogleGenAIToolset returns the correct tools."""
    toolset = google_genai.GoogleGenAIToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 3
    assert tools[0] is google_genai.get_info_about_youtube_video
    assert tools[1] is google_genai.generate_video
    assert tools[2] is google_genai.generate_image


@pytest.mark.asyncio
@mock.patch("google.genai.Client")
async def test_module_initialization_vertexai(
    mock_genai_client: mock.Mock,
):
    """Tests that the genai.Client is initialized correctly with Vertex AI."""
    with mock.patch.dict(
        "os.environ",
        {
            "GOOGLE_GENAI_USE_VERTEXAI": "TRUE",
            "GOOGLE_CLOUD_PROJECT": "test-project",
            "GOOGLE_CLOUD_LOCATION": "test-location",
        },
        clear=True,
    ):
        # Reload the module to trigger initialization with the new env vars
        _ = importlib.reload(google_genai)
        mock_genai_client.assert_called_with(
            vertexai=True,
            project="test-project",
            location="test-location",
        )


@pytest.mark.asyncio
@mock.patch("google.genai.Client")
async def test_module_initialization_no_vertexai(
    mock_genai_client: mock.Mock,
):
    """Tests that the genai.Client is initialized without Vertex AI."""
    with mock.patch.dict(
        "os.environ",
        {
            "GOOGLE_GENAI_USE_VERTEXAI": "FALSE",
            "GOOGLE_CLOUD_PROJECT": "test-project",
            "GOOGLE_CLOUD_LOCATION": "test-location",
        },
        clear=True,
    ):
        # Reload the module to trigger initialization with the new env vars
        _ = importlib.reload(google_genai)
        mock_genai_client.assert_called_with(
            vertexai=False,
            project="test-project",
            location="test-location",
        )
