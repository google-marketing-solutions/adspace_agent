# Copyright 2024 Google LLC
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

"""Tools for analyzing organic youtube content."""

import os

from google import genai
from googleapiclient.discovery import build


def get_relevant_youtube_videos(search_query: str, n: int = 5) -> list[str]:
  """Returns a list of urls for organic youtube videos relevant to the search query."""

  video_url = "https://www.youtube.com/watch?v="

  youtube = build(
      "youtube", "v3", developerKey=os.environ["YOUTUBE_DATA_API_KEY"]
  )

  request = youtube.search().list(
      part="snippet", type="video", q=search_query, maxResults=n
  )

  response = request.execute()

  video_ids = [
      f"{video_url}{item['id']['videoId']}" for item in response["items"]
  ]

  return video_ids


def get_youtube_channel_subscriber_count(channel_id: str) -> str:
  """Retrieves the subscriber count for a given YouTube channel ID.

  This function uses the YouTube Data API v3 to access the channel's
  public statistics.

  Args:
      channel_id (str): The ID of the YouTube channel.

  Returns:
      int: The subscriber count as an integer, otherwise None.
  """
  try:

    youtube = build(
        "youtube", "v3", developerKey=os.environ["YOUTUBE_DATA_API_KEY"]
    )

    request = youtube.channels().list(part="statistics", id=channel_id)
    response = request.execute()

    if (
        not response.get("items")
        or "subscriberCount" not in response["items"][0]["statistics"]
    ):
      print(
          "Error: Could not retrieve subscriber count for channel"
          f" '{channel_id}'."
      )
      return None

    subscriber_count = int(
        response["items"][0]["statistics"]["subscriberCount"]
    )
    return subscriber_count

  except Exception as e:
    print(f"An error occurred: {e}")
    return None


def get_channel_id_from_youtube_video(video_id: str) -> str:
  """Retrieves the channel ID for a given YouTube video ID.

  Args:
      video_id (str): The ID of the YouTube video.

  Returns:
      str: The channel ID if found, otherwise None.
  """
  try:

    youtube = build(
        "youtube", "v3", developerKey=os.environ["YOUTUBE_DATA_API_KEY"]
    )

    request = youtube.videos().list(part="snippet", id=video_id)

    response = request.execute()

    if not response.get("items"):
      print(f"Error: No video found with ID '{video_id}'.")
      return None

    channel_id = response["items"][0]["snippet"]["channelId"]
    return channel_id

  except Exception as e:
    print(f"An error occurred: {e}")
    return None


def get_channel_name(channel_id: str) -> str:
  """Retrieves the name of a YouTube channel given its channel ID.

  Args:
      channel_id (str): The ID of the YouTube channel.

  Returns:
      str: The channel name if found, otherwise None.
  """
  try:

    youtube = build(
        "youtube", "v3", developerKey=os.environ["YOUTUBE_DATA_API_KEY"]
    )

    request = youtube.channels().list(part="snippet", id=channel_id)
    response = request.execute()

    if not response.get("items"):
      print(f"Error: No channel found with ID '{channel_id}'.")
      return None

    channel_name = response["items"][0]["snippet"]["title"]
    return channel_name

  except Exception as e:
    print(f"An error occurred: {e}")
    return None


def summarize_youtube_videos(video_urls: list[str], prompt: str) -> list[str]:
  """Summarize YouTube videos using the urls and returns a list of summaries."""

  genai_client = genai.Client(
      api_key=os.environ["GOOGLE_API_KEY"],
      vertexai=False
  )

  summaries = []

  for video_url in video_urls:

    response = genai_client.models.generate_content(
        model=os.environ["GEMINI_MODEL"],
        contents=genai.types.Content(
            parts=[
                genai.types.Part(
                    file_data=genai.types.FileData(file_uri=video_url)
                ),
                genai.types.Part(text=prompt),
            ]
        ),
    )

    summaries.append(response.text)

  return summaries
