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
"""Tests for the skills toolset."""

from unittest.mock import AsyncMock
from unittest.mock import MagicMock
from unittest.mock import patch

from google.adk.skills import Frontmatter
from google.adk.skills import Resources
from google.adk.skills import Skill
from google.adk.tools import FunctionTool
from google.adk.tools.skill_toolset import SkillToolset
import pytest

from adspace_agent.tools.skills import SkillsToolset


@pytest.mark.asyncio
async def test_skills_toolset_initialization(monkeypatch, tmp_path):
    """Tests that SkillsToolset initializes with skills loaded from env."""
    monkeypatch.setenv("LOCAL_SKILLS_DIR", str(tmp_path))
    mock_frontmatter = Frontmatter(
        name="test-skill", description="A test skill description."
    )
    mock_skill = Skill(
        frontmatter=mock_frontmatter,
        instructions="Test instructions",
        resources=Resources(),
    )

    with (
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            return_value={"test-skill": mock_frontmatter},
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_dir",
            return_value=mock_skill,
        ),
    ):
        toolset = SkillsToolset()
        assert isinstance(toolset.skills_toolset, SkillToolset)
        assert len(toolset.skills_toolset._skills) == 1  # ruff:ignore[private-member-access]
        assert "test-skill" in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]

        tools = await toolset.get_tools()
        # Should return core skill tools (ListSkillsTool, LoadSkillTool,
        # LoadSkillResourceTool, RunSkillScriptTool) plus reload_skills_tool.
        expected_tool_count = 5
        assert len(tools) == expected_tool_count

        reload_tools = [t for t in tools if t.name == "reload_skills"]
        assert len(reload_tools) == 1
        assert isinstance(reload_tools[0], FunctionTool)


@pytest.mark.asyncio
async def test_skills_toolset_reload(monkeypatch, tmp_path):
    """Tests that SkillsToolset successfully reloads skills in-place."""
    monkeypatch.setenv("LOCAL_SKILLS_DIR", str(tmp_path))
    mock_frontmatter_1 = Frontmatter(
        name="skill-1", description="Description 1"
    )
    mock_skill_1 = Skill(
        frontmatter=mock_frontmatter_1,
        instructions="Instructions 1",
        resources=Resources(),
    )

    mock_frontmatter_2 = Frontmatter(
        name="skill-2", description="Description 2"
    )
    mock_skill_2 = Skill(
        frontmatter=mock_frontmatter_2,
        instructions="Instructions 2",
        resources=Resources(),
    )

    with (
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            side_effect=[
                {"skill-1": mock_frontmatter_1},
                {"skill-2": mock_frontmatter_2},
            ],
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_dir",
            side_effect=[mock_skill_1, mock_skill_2],
        ),
    ):
        toolset = SkillsToolset()
        assert "skill-1" in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]
        assert "skill-2" not in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]

        tools = await toolset.get_tools()
        reload_tool = next(t for t in tools if t.name == "reload_skills")
        assert isinstance(reload_tool, FunctionTool)

        # Trigger reload
        result = reload_tool.func()
        assert "Successfully reloaded skills" in result

        # Verify it now has skill-2 instead of skill-1
        assert "skill-1" not in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]
        assert "skill-2" in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]


@pytest.mark.asyncio
async def test_skills_toolset_reload_with_errors(monkeypatch, tmp_path):
    """Tests that SkillsToolset reload reports loading errors to the user."""
    monkeypatch.setenv("LOCAL_SKILLS_DIR", str(tmp_path))
    mock_frontmatter_1 = Frontmatter(
        name="skill-1", description="Description 1"
    )
    mock_skill_1 = Skill(
        frontmatter=mock_frontmatter_1,
        instructions="Instructions 1",
        resources=Resources(),
    )

    with (
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            side_effect=[
                {"skill-1": mock_frontmatter_1, "bad-skill": None},
                {"skill-1": mock_frontmatter_1, "bad-skill": None},
            ],
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_dir",
            side_effect=[
                mock_skill_1,
                ValueError("Invalid frontmatter configuration"),
                mock_skill_1,
                ValueError("Invalid frontmatter configuration"),
            ],
        ),
    ):
        toolset = SkillsToolset()
        assert "skill-1" in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]
        assert "bad-skill" not in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]

        tools = await toolset.get_tools()
        reload_tool = next(t for t in tools if t.name == "reload_skills")
        assert isinstance(reload_tool, FunctionTool)

        # Trigger reload
        result = reload_tool.func()
        assert "Successfully reloaded skills" in result
        assert "WARNING: The following errors occurred" in result
        assert (
            "Local skill 'bad-skill' failed to load: "
            "Invalid frontmatter configuration"
        ) in result

        # Verify skill-1 is still there, but bad-skill is not
        assert "skill-1" in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]
        assert "bad-skill" not in toolset.skills_toolset._skills  # ruff:ignore[private-member-access]


@pytest.mark.asyncio
async def test_skills_toolset_gcs_and_local_errors_and_lifecycle(
    monkeypatch, tmp_path
):
    """Tests GCS/local error paths, process_llm_request, and close."""
    monkeypatch.setenv("SKILLS_BUCKET_NAME", "test-bucket")
    monkeypatch.setenv("LOCAL_SKILLS_DIR", str(tmp_path))

    with (
        patch(
            "adspace_agent.tools.skills.list_skills_in_gcs_dir",
            side_effect=[
                RuntimeError("GCS list error"),
                {"bad-gcs-skill": None},
            ],
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_gcs_dir",
            side_effect=RuntimeError("GCS load error"),
        ),
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            side_effect=RuntimeError("Local list error"),
        ),
    ):
        toolset = SkillsToolset()
        res = toolset.reload_skills()
        assert "GCS skill 'bad-gcs-skill' failed to load" in res
        assert "Failed to list local skills" in res

        toolset.skills_toolset.process_llm_request = AsyncMock()
        toolset.skills_toolset.close = AsyncMock()
        mock_ctx = MagicMock()
        mock_req = MagicMock()
        await toolset.process_llm_request(
            tool_context=mock_ctx, llm_request=mock_req
        )
        toolset.skills_toolset.process_llm_request.assert_awaited_once_with(
            tool_context=mock_ctx, llm_request=mock_req
        )
        await toolset.close()
        toolset.skills_toolset.close.assert_awaited_once()
