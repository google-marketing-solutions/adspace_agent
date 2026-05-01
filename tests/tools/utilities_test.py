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
"""Tests for the utilities tools."""

from unittest import mock

from google.adk.tools.tool_context import ToolContext
import pytest

from adspace_agent.tools import utilities


@pytest.mark.asyncio
async def test_list_user_files_success() -> None:
    """Tests that list_user_files returns the list of artifacts."""
    mock_context = mock.AsyncMock(spec=ToolContext)
    mock_context.list_artifacts.return_value = ["file1.txt", "file2.txt"]

    result: str = await utilities.list_user_files(mock_context)

    assert "Here are your available artifacts:" in result
    assert "- file1.txt" in result
    assert "- file2.txt" in result


@pytest.mark.asyncio
async def test_list_user_files_empty() -> None:
    """Tests that list_user_files handles empty artifact list."""
    mock_context = mock.AsyncMock(spec=ToolContext)
    mock_context.list_artifacts.return_value = []

    result: str = await utilities.list_user_files(mock_context)

    assert result == "You have no saved artifacts."


@pytest.mark.asyncio
async def test_list_user_files_value_error() -> None:
    """Tests that list_user_files handles ValueError."""
    mock_context = mock.AsyncMock(spec=ToolContext)
    mock_context.list_artifacts.side_effect = ValueError("Config error")

    result: str = await utilities.list_user_files(mock_context)

    assert result == "Error: Could not list artifacts."


@pytest.mark.asyncio
async def test_list_user_files_exception() -> None:
    """Tests that list_user_files handles generic exceptions."""
    mock_context = mock.AsyncMock(spec=ToolContext)
    mock_context.list_artifacts.side_effect = Exception("Unexpected error")

    result: str = await utilities.list_user_files(mock_context)

    assert (
        result == "Error: An unexpected error occurred while listing artifacts."
    )


@pytest.mark.asyncio
async def test_utilities_toolset() -> None:
    """Tests that the UtilitiesToolset returns the correct tools."""
    toolset = utilities.UtilitiesToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 1
    assert tools[0] is utilities.list_files_tool
