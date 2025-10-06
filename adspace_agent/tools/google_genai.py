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
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import BaseTool
from google.adk.tools import FunctionTool
from google.adk.tools.base_toolset import BaseToolset


@FunctionTool
def get_info_about_youtube_video(
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

        response = genai_client.models.generate_content(
            model="gemini-2.5-pro", contents=[contents]
        )

        return {"status": "SUCCESS", "response": response.text}

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
        return [get_info_about_youtube_video]
