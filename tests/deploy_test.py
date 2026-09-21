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
"""Testing the deploy module."""

import pathlib
from unittest.mock import MagicMock
from unittest.mock import patch

from adspace_agent import deploy


def test_load_requirements(tmp_path: pathlib.Path):
    """Test _load_requirements parses packages and ignores comments/flags."""
    req_file = tmp_path / "requirements.txt"
    req_file.write_text(
        "# Comment line\n"
        "\n"
        "-i https://pypi.org/simple\n"
        "google-adk>=1.0.0  # inline comment\n"
        "pydantic==2.10.0\n",
        encoding="utf-8",
    )
    reqs = deploy._load_requirements(req_file)  # ruff:ignore[private-member-access]
    assert reqs == ["google-adk>=1.0.0", "pydantic==2.10.0"]


def test_main_updates_existing_engine():
    """Test main updates an existing Agent Engine when found."""
    mock_existing = MagicMock()
    mock_existing.display_name = "AdSpace Agent"
    mock_existing.resource_name = "projects/p/locations/l/reasoningEngines/1"

    mock_updated = MagicMock()
    mock_updated.resource_name = mock_existing.resource_name

    with (
        patch("adspace_agent.deploy.vertexai.init") as mock_init,
        patch("adspace_agent.deploy.create_agent") as mock_create_agent,
        patch("adspace_agent.deploy.agent_engines.AdkApp"),
        patch(
            "adspace_agent.deploy._load_requirements",
            return_value=["google-adk"],
        ),
        patch(
            "adspace_agent.deploy.agent_engines.list",
            return_value=[mock_existing],
        ),
        patch(
            "adspace_agent.deploy.agent_engines.update",
            return_value=mock_updated,
        ) as mock_update,
        patch("adspace_agent.deploy.agent_engines.create") as mock_create,
    ):
        deploy.main()
        mock_init.assert_called_once()
        mock_create_agent.assert_called_once()
        mock_update.assert_called_once()
        mock_create.assert_not_called()


def test_main_creates_new_engine_when_list_fails():
    """Test main creates a new Agent Engine when none exists or list raises."""
    mock_created = MagicMock()
    mock_created.resource_name = "projects/p/locations/l/reasoningEngines/2"

    with (
        patch("adspace_agent.deploy.vertexai.init") as mock_init,
        patch("adspace_agent.deploy.create_agent") as mock_create_agent,
        patch("adspace_agent.deploy.agent_engines.AdkApp"),
        patch(
            "adspace_agent.deploy._load_requirements",
            return_value=["google-adk"],
        ),
        patch(
            "adspace_agent.deploy.agent_engines.list",
            side_effect=RuntimeError("API error"),
        ),
        patch("adspace_agent.deploy.agent_engines.update") as mock_update,
        patch(
            "adspace_agent.deploy.agent_engines.create",
            return_value=mock_created,
        ) as mock_create,
    ):
        deploy.main()
        mock_init.assert_called_once()
        mock_create_agent.assert_called_once()
        mock_update.assert_not_called()
        mock_create.assert_called_once()
