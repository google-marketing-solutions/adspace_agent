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
"""Toolset for an AI to ping specific Genai endpoints in Google Cloud."""

import os
from typing import override

from google import genai
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import BaseTool
from google.adk.tools import FunctionTool
from google.adk.tools.base_toolset import BaseToolset


@FunctionTool
def get_info_about_youtube_video(
    external_video_id: str, prompt: str
) -> dict[str, str]:
  """Passed video url to Gemini to answer the prompt.

  This function is used to give an AI the ability to answer
  questions about a video, be it visual, audio or both.

  Args:
    external_video_id:  External video id, found at the end of a YouTube url.
    prompt: The prompt used to summarize the video.

  Returns:
    Dictioanry with a status.  If the call failed, the error will be
    returned in the error_details field.  If the call succeeded, the
    response to the prompt will be returned in the response field.
  """

  try:

    video_url = "https://www.youtube.com/watch?v="

    genai_client = genai.Client(
        vertexai=True,
        project=os.environ["GOOGLE_CLOUD_PROJECT"],
        location=os.environ["GOOGLE_CLOUD_LOCATION"],
    )

    contents = genai.types.Content(
        parts=[
            genai.types.Part(
                file_data=genai.types.FileData(
                    file_uri=f"{video_url}{external_video_id}",
                    mime_type="video/mp4",
                )
            ),
            genai.types.Part(text=prompt),
        ],
        role="user",
    )

    response = genai_client.models.generate_content(
        model=os.environ["GEMINI_MODEL"], contents=[contents]
    )

    return {"status": "SUCCESS", "response": response.text}

  except Exception as e:

    return {
        "status": "ERROR",
        "error_details": str(e),
    }


class GoogleGenaiToolset(BaseToolset):
  """A custom toolset for calling Google Genai APIs."""

  @override
  async def get_tools(
      self,
      readonly_context: ReadonlyContext | None = None,
  ) -> list[BaseTool]:

    return [get_info_about_youtube_video]
