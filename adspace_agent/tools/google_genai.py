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
"""A set of tools for the AdSpace Agent to interact with Google Cloud GenAI."""

import os
from typing import override

from google import genai
from google.adk.agents.callback_context import CallbackContext
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import BaseTool
from google.adk.tools import FunctionTool
from google.adk.tools.base_toolset import BaseToolset
import google.genai.types as types
from google.genai.types import Part


@FunctionTool
async def get_info_about_youtube_video(
    youtube_video_id: str, prompt: str
) -> dict[str, str | None]:
    """Gets info about a YouTube video based on a prompt.

    This function gives the ability to prompt a YouTube video and ask questions
    about the video's visual, audio, and more. It also provides answers to the
    prompt.

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

        genai_client = genai.Client(
            vertexai=True,
            project=os.environ["GOOGLE_CLOUD_PROJECT"],
            location=os.environ["GOOGLE_CLOUD_LOCATION"],
        )

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
            model="gemini-2.5-pro", contents=[contents]
        )

        return {"status": "SUCCESS", "response": response.text}
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


@FunctionTool
async def generate_video(
    prompt: str,
    filename: str,
    tool_context: CallbackContext,
) -> dict[str, str | Part]:
    """Generates a video from a prompt using Google GenAI' Veo 3 model.

    Args:
        prompt (str): The prompt to use for creating the video.
        filename (str): The filename to use for the generated video.
        tool_context (CallbackContext): The callback context.

    Returns:
        dict[str, str | Part]: A dictionary containing the status of the
            operation and the response.
    """
    try:
        client = genai.Client(
            vertexai=True,
            project=os.environ["GOOGLE_CLOUD_PROJECT"],
            location=os.environ["GOOGLE_CLOUD_LOCATION"],
        )

        operation = await client.aio.models.generate_videos(
            model="veo-3.0-generate-001",
            prompt=prompt,
        )

        if not operation.response:
            return {
                "status": "ERROR",
                "error_details": "Video generation failed: No response.",
            }

        if not operation.response.generated_videos:
            return {
                "status": "ERROR",
                "error_details": (
                    "Video generation failed: No videos generated."
                ),
            }

        generated_video = operation.response.generated_videos[0]
        if not generated_video.video:
            return {
                "status": "ERROR",
                "error_details": (
                    "Video generation failed: Missing video data."
                ),
            }

        video_bytes = generated_video.video.video_bytes
        mime_type = generated_video.video.mime_type

        if not video_bytes or not mime_type:
            return {
                "status": "ERROR",
                "error_details": (
                    "Video generation failed: Empty video content."
                ),
            }

        video_artifact = types.Part(
            inline_data=types.Blob(mime_type=mime_type, data=video_bytes)
        )

        version = await tool_context.save_artifact(
            filename=filename,
            artifact=video_artifact,
        )

        return {
            "status": "SUCCESS",
            "message": f"Generated video: '{filename}' (version: {version}).",
        }
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


@FunctionTool
async def generate_image(
    prompt: str,
    filename: str,
    tool_context: CallbackContext,
) -> dict[str, str | Part]:
    """Generates an image from a prompt using Google GenAI's Imagen 3 model.

    Args:
        prompt (str): The prompt to use for creating the image.
        filename (str): The filename to use for the generated image.
        tool_context (CallbackContext): The callback context.

    Returns:
        dict[str, str | Part]: A dictionary containing the status of the
            operation and the response.
    """
    try:
        client = genai.Client(
            vertexai=True,
            project=os.environ["GOOGLE_CLOUD_PROJECT"],
            location=os.environ["GOOGLE_CLOUD_LOCATION"],
        )

        response = await client.aio.models.generate_images(
            model="imagen-3.0-generate-001",
            prompt=prompt,
        )

        if not response.generated_images:
            return {
                "status": "ERROR",
                "error_details": (
                    "Image generation failed: No images generated."
                ),
            }

        generated_image = response.generated_images[0]
        if not generated_image.image:
            return {
                "status": "ERROR",
                "error_details": (
                    "Image generation failed: Missing image data."
                ),
            }

        image_bytes = generated_image.image.image_bytes
        mime_type = generated_image.image.mime_type

        if not image_bytes or not mime_type:
            return {
                "status": "ERROR",
                "error_details": (
                    "Image generation failed: Empty image content."
                ),
            }

        image_artifact = types.Part(
            inline_data=types.Blob(mime_type=mime_type, data=image_bytes)
        )

        version = await tool_context.save_artifact(
            filename=filename,
            artifact=image_artifact,
        )

        return {
            "status": "SUCCESS",
            "message": f"Generated image: '{filename}' (version: {version}).",
        }
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


class GoogleGenAIToolset(BaseToolset):
    """A custom toolset for calling Google GenAI APIs."""

    @override
    async def get_tools(  # pytype: disable=override-error
        self,
        readonly_context: ReadonlyContext | None = None,  # pylint: disable=unused-argument
    ) -> list[BaseTool]:
        return [get_info_about_youtube_video, generate_video, generate_image]
