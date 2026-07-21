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
"""A set of tools for the AdSpace Agent to interact with Google Cloud GenAI."""

import asyncio
import os
from typing import override
import uuid

from google import genai
from google.adk.agents.callback_context import CallbackContext
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import FunctionTool
from google.adk.tools import LongRunningFunctionTool
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset
from google.genai import types
from google.genai.types import Part

MODEL: str = os.environ.get("MODEL", "gemini-3.1-flash-lite")
VEO_MODEL: str = os.environ.get("VEO_MODEL", "veo-3.1-fast-generate-001")
IMAGEN_MODEL: str = os.environ.get(
    "IMAGEN_MODEL", "imagen-4.0-fast-generate-001"
)

genai_client = genai.Client(
    vertexai=os.getenv("GOOGLE_GENAI_USE_VERTEXAI", "False").upper() == "TRUE",
    project=os.environ["GOOGLE_CLOUD_PROJECT"],
    location=os.environ["GOOGLE_CLOUD_LOCATION"],
)


@FunctionTool
async def get_info_about_youtube_video(
    youtube_video_id: str, prompt: str
) -> dict[str, str | None]:
    """Gets info about a YouTube video based on a prompt.

    This function gives the ability to prompt a YouTube video and ask questions
    about the video's visual, audio, and more. It also provides answers to the
    prompt.

    This function is optimized for parallel execution - call multiple times
    for different videos if needed.

    Args:
      youtube_video_id: The external YouTube video ID, found at the end of a
        YouTube URL in the `v` parameter. For example, in the URL
        `https://www.youtube.com/watch?v=abc123`, the `youtube_video_id` would
        be `abc123`.
      prompt: The prompt used to ask the YouTube video questions about itself.

    Returns:
      Dictionary with a status. If the call failed, the error will be returned
      in the error_details field. If the call succeeded, the response to the
      prompt will be returned in the response field.
    """
    try:
        youtube_video_url = "https://www.youtube.com/watch?v="

        contents = genai.types.Content(
            parts=[
                genai.types.Part(
                    file_data=genai.types.FileData(
                        file_uri=f"{youtube_video_url}{youtube_video_id}",
                        mime_type="video/mp4",
                    )
                ),
                genai.types.Part(text=prompt),
            ],
            role="user",
        )

        response = await genai_client.aio.models.generate_content(
            model=MODEL, contents=[contents]
        )
    except Exception as ex:  # ruff:ignore[blind-except]
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }
    else:
        return {"status": "SUCCESS", "response": response.text}


@LongRunningFunctionTool
async def generate_video(
    prompt: str,
    tool_context: CallbackContext,
) -> dict[str, str | Part]:
    """Generates a video from a prompt using Google GenAI' Veo 3 model.

    This function is optimized for parallel execution.

    Args:
        prompt (str): The prompt to use for creating the video.
        tool_context (CallbackContext): The callback context.

    Returns:
        dict[str, Union[str, Part]]: A dictionary containing the status of the
            operation and the response.
    """
    result: dict[str, str | Part] = {}
    try:
        operation = await genai_client.aio.models.generate_videos(
            model=VEO_MODEL,
            source=types.GenerateVideosSource(
                prompt=prompt,
            ),
            config=types.GenerateVideosConfig(number_of_videos=1),
        )

        while not operation.done:
            await asyncio.sleep(5)
            operation = await genai_client.aio.operations.get(operation)
    except Exception as ex:  # ruff:ignore[blind-except]
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }

    if not operation.response:
        result = {
            "status": "ERROR",
            "error_details": "Video generation failed: No response.",
        }
    elif not operation.response.generated_videos:
        result = {
            "status": "ERROR",
            "error_details": "Video generation failed: No videos generated.",
        }
    elif not operation.response.generated_videos[0].video:
        result = {
            "status": "ERROR",
            "error_details": "Video generation failed: Missing video data.",
        }
    else:
        generated_video = operation.response.generated_videos[0]
        video = generated_video.video
        assert video is not None  # ruff:ignore[assert]
        video_bytes = video.video_bytes
        mime_type = video.mime_type

        if not video_bytes or not mime_type:
            result = {
                "status": "ERROR",
                "error_details": (
                    "Video generation failed: Empty video content."
                ),
            }
        else:
            artifact = types.Part(
                inline_data=types.Blob(mime_type=mime_type, data=video_bytes)
            )
            filename = f"generated_video_{uuid.uuid4().hex}.mp4"
            try:
                _ = await tool_context.save_artifact(
                    filename=filename,
                    artifact=artifact,
                )
                result = {
                    "status": "SUCCESS",
                    "message": "Generated video.",
                }
            except Exception as ex:  # ruff:ignore[blind-except]
                result = {
                    "status": "ERROR",
                    "error_details": str(ex),
                }

    return result


@LongRunningFunctionTool
async def generate_image(
    prompt: str,
    tool_context: CallbackContext,
) -> dict[str, str | Part]:
    """Generates an image from a prompt using Google GenAI's Imagen 3 model.

    This function is optimized for parallel execution.

    Args:
        prompt (str): The prompt to use for creating the image.
        tool_context (CallbackContext): The callback context.

    Returns:
        dict[str, Union[str, Part]]: A dictionary containing the status of the
            operation and the response.
    """
    result: dict[str, str | Part] = {}
    try:
        response = await genai_client.aio.models.generate_images(
            model=IMAGEN_MODEL,
            prompt=prompt,
            config=types.GenerateImagesConfig(number_of_images=1),
        )
    except Exception as ex:  # ruff:ignore[blind-except]
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }

    if not response.generated_images:
        result = {
            "status": "ERROR",
            "error_details": "Image generation failed: No images generated.",
        }
    elif not response.generated_images[0].image:
        result = {
            "status": "ERROR",
            "error_details": "Image generation failed: Missing image data.",
        }
    else:
        generated_image = response.generated_images[0]
        image = generated_image.image
        assert image is not None  # ruff:ignore[assert]
        image_bytes = image.image_bytes
        mime_type = image.mime_type

        if not image_bytes or not mime_type:
            result = {
                "status": "ERROR",
                "error_details": (
                    "Image generation failed: Empty image content."
                ),
            }
        else:
            artifact = types.Part(
                inline_data=types.Blob(mime_type=mime_type, data=image_bytes)
            )
            filename = f"generated_image_{uuid.uuid4().hex}.png"
            try:
                _ = await tool_context.save_artifact(
                    filename=filename,
                    artifact=artifact,
                )
                result = {
                    "status": "SUCCESS",
                    "message": "Generated image.",
                }
            except Exception as ex:  # ruff:ignore[blind-except]
                result = {
                    "status": "ERROR",
                    "error_details": str(ex),
                }

    return result


class GoogleGenAIToolset(BaseToolset):
    """A custom toolset for calling Google GenAI APIs."""

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        return [get_info_about_youtube_video, generate_video, generate_image]
