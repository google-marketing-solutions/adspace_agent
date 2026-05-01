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
"""Tests for list_tools.py."""

import subprocess  # noqa: S404
import sys
from unittest.mock import AsyncMock
from unittest.mock import MagicMock
from unittest.mock import patch

import pytest

from adspace_agent.utils.list_tools import list_tools
from adspace_agent.utils.list_tools import main


def test_list_tools_sync_objects(capsys: pytest.CaptureFixture[str]) -> None:
    """Test list_tools with objects having .name attribute."""
    tool1 = MagicMock()
    tool1.name = "apple"
    tool2 = MagicMock()
    tool2.name = "banana"

    list_tools([tool2, tool1])

    captured = capsys.readouterr()
    assert captured.out == "apple\nbanana\n"


def test_list_tools_dicts(capsys: pytest.CaptureFixture[str]) -> None:
    """Test list_tools with dicts."""
    list_tools([{"name": "apple"}, {"name": "banana"}])

    captured = capsys.readouterr()
    assert captured.out == "apple\nbanana\n"


def test_list_tools_fallback(capsys: pytest.CaptureFixture[str]) -> None:
    """Test list_tools fallback to str()."""
    list_tools(["apple", "banana"])

    captured = capsys.readouterr()
    assert captured.out == "apple\nbanana\n"


@patch("adspace_agent.utils.list_tools.DiscoveryConverter")
def test_main_google_ads(
    mock_converter_class: MagicMock, capsys: pytest.CaptureFixture[str]
) -> None:
    """Test main with ads subcommand."""
    mock_converter = MagicMock()
    mock_converter_class.return_value = mock_converter
    mock_converter.convert.return_value = {
        "paths": {
            "/path1": {"get": {"operationId": "apple"}},
            "/path2": {"post": {"operationId": "banana"}},
        }
    }

    with patch.object(sys, "argv", ["list-tools", "google_ads"]):
        main()

    captured = capsys.readouterr()
    assert "apple" in captured.out
    assert "banana" in captured.out


@patch("adspace_agent.utils.list_tools.YoutubeToolset")
def test_main_youtube(
    mock_youtube_class: MagicMock, capsys: pytest.CaptureFixture[str]
) -> None:
    """Test main with youtube subcommand."""
    mock_toolset = MagicMock()
    mock_youtube_class.return_value = mock_toolset

    tool1 = MagicMock()
    tool1.name = "youtube_tool"

    mock_toolset.get_tools = AsyncMock(return_value=[tool1])

    with patch.object(sys, "argv", ["list-tools", "youtube"]):
        main()

    captured = capsys.readouterr()
    assert "youtube_tool" in captured.out


@patch("adspace_agent.utils.list_tools.GoogleApiToolset")
def test_main_google(
    mock_google_class: MagicMock, capsys: pytest.CaptureFixture[str]
) -> None:
    """Test main with google subcommand."""
    mock_toolset = MagicMock()
    mock_google_class.return_value = mock_toolset

    tool1 = MagicMock()
    tool1.name = "google_tool"

    mock_toolset.get_tools = AsyncMock(return_value=[tool1])

    with patch.object(
        sys,
        "argv",
        ["list-tools", "google", "--api", "drive", "--version", "v3"],
    ):
        main()

    captured = capsys.readouterr()
    assert "google_tool" in captured.out


def test_main_invalid_subcommand() -> None:
    """Test main with invalid subcommand."""
    with (
        patch.object(sys, "argv", ["list-tools", "invalid"]),
        pytest.raises(SystemExit),
    ):
        main()


def test_main_as_script() -> None:
    """Test running list_tools.py as a script."""
    result = subprocess.run(
        [sys.executable, "-m", "adspace_agent.utils.list_tools", "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0
    assert "List tools for various Google APIs." in result.stdout
