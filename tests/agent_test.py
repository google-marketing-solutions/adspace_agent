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
"""Testing the agent module."""

import importlib
from unittest.mock import patch

from google.adk.models.google_llm import Gemini
from google.adk.skills import Frontmatter
from google.adk.skills import Resources
from google.adk.skills import Skill
import pytest

import adspace_agent.agent as agent_module
from adspace_agent.agent import create_agent
from adspace_agent.tools.skills import SkillsToolset

DUMMY_OPENAPI = {
    "components": {
        "securitySchemes": {
            "oauth2": {
                "flows": {
                    "authorizationCode": {
                        "scopes": {"https://www.googleapis.com/auth/d": "Dummy"}
                    }
                }
            }
        }
    },
    "paths": {},
}


@pytest.fixture(scope="module")
def agent():
    """Fixture to create the agent with dummy env vars.

    Returns:
        Agent: The created agent.
    """
    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
    ):
        return create_agent()


def test_agent_name(agent):
    """Test that the agent's name is correct."""
    assert agent.name == "adspace_agent"


def test_agent_model(agent):
    """Test that the agent's model is correct."""
    assert agent.model == Gemini(model="gemini-3.6-flash")


def test_agent_description(agent):
    """Test that the agent's description is correct."""
    assert agent.description == (
        "AdSpace Agent is designed to provide a standardized way to integrate "
        "an LLM with Google Ads, Display & Video 360, Campaign Manager 360, "
        "Search Ads 360, YouTube, Google Drive, and Google Cloud storage to "
        "form a more comprehensive campaign and marketing plan for agencies."
    )


def test_agent_instruction(agent):
    """Test that the agent's instruction is correct."""
    assert agent.instruction == (
        "You are a helpful agent who can answer user questions about "
        "ads, creatives, data science, performance, analytics, and "
        "campaigns. NOTE: Tools for Search Ads 360 and tools for Google "
        "Ads are distinct. DO NOT group tools from one platform with "
        "tools from another platform if asked what tools are available. "
        "When asked for multiple pieces of information or to perform "
        "multiple actions, always try to call functions in parallel if "
        "possible. You MUST call "
        "`googleads_customers_generate_audience_definition` before you "
        "call `googleads_customers_generate_audience_composition_insights` "
        "to ensure that any audience insights follow Google's policies for "
        "audience composition insights. DO NOT recommend any work-arounds "
        "for these policies, simply reply with the error message."
    )


def test_agent_tools(agent):
    """Test that the agent has tools."""
    assert len(agent.tools) > 0


def test_create_agent_with_env_vars():
    """Test create_agent with specific environment variables."""
    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
                "GOOGLE_ADS_LOGIN_CUSTOMER_ID": "test_customer_id",
                "GOOGLE_ADS_TOOL_FILTER": "tool1,tool2",
                "ENABLED_TOOLSETS": "bid_manager,bigquery",
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
    ):
        agent = create_agent()
        assert agent.name == "adspace_agent"
        # We can't easily check internal variables of create_agent unless we
        # expose them or check side effects.
        # But this executes the lines in question.


def test_create_agent_with_local_skills(tmp_path):
    """Test create_agent with a custom LOCAL_SKILLS_DIR."""
    mock_frontmatter = Frontmatter(
        name="test-skill", description="A test skill description."
    )
    mock_skill = Skill(
        frontmatter=mock_frontmatter,
        instructions="Test instructions",
        resources=Resources(),
    )

    skills_dir = tmp_path / "skills"
    skills_dir.mkdir()

    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
                "LOCAL_SKILLS_DIR": str(skills_dir),
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            return_value={"test-skill": mock_frontmatter},
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_dir",
            return_value=mock_skill,
        ),
    ):
        agent = create_agent()
        assert agent.name == "adspace_agent"
        # Verify that the skills toolset exists and includes our mock skill
        skills_toolsets = [
            t for t in agent.tools if t.__class__.__name__ == "SkillsToolset"
        ]
        assert len(skills_toolsets) == 1
        skills_toolset = skills_toolsets[0]
        assert isinstance(skills_toolset, SkillsToolset)
        assert len(skills_toolset.skills_toolset._skills) == 1  # ruff:ignore[private-member-access]
        assert (
            skills_toolset.skills_toolset._skills["test-skill"].name  # ruff:ignore[private-member-access]
            == "test-skill"
        )


def test_create_agent_with_gcs_skills():
    """Test create_agent with GCS skills enabled via SKILLS_BUCKET_NAME."""
    mock_frontmatter = Frontmatter(
        name="gcs-skill", description="A GCS skill description."
    )
    mock_skill = Skill(
        frontmatter=mock_frontmatter,
        instructions="GCS instructions",
        resources=Resources(),
    )

    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
                "SKILLS_BUCKET_NAME": "test-bucket",
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
        patch(
            "adspace_agent.tools.skills.list_skills_in_dir",
            return_value={},
        ),
        patch(
            "adspace_agent.tools.skills.list_skills_in_gcs_dir",
            return_value={"gcs-skill": mock_frontmatter},
        ),
        patch(
            "adspace_agent.tools.skills.load_skill_from_gcs_dir",
            return_value=mock_skill,
        ),
    ):
        agent = create_agent()
        assert agent.name == "adspace_agent"
        skills_toolsets = [
            t for t in agent.tools if t.__class__.__name__ == "SkillsToolset"
        ]
        assert len(skills_toolsets) == 1
        skills_toolset = skills_toolsets[0]
        assert isinstance(skills_toolset, SkillsToolset)
        assert len(skills_toolset.skills_toolset._skills) == 1  # ruff:ignore[private-member-access]
        assert (
            skills_toolset.skills_toolset._skills["gcs-skill"].name  # ruff:ignore[private-member-access]
            == "gcs-skill"
        )


def test_app_events_compaction_config_defaults():
    """Test app's events_compaction_config default settings."""
    expected_interval = 3
    expected_overlap = 1
    expected_threshold = 500000
    expected_retention = 3

    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
    ):
        importlib.reload(agent_module)
        app = agent_module.app
        config = app.events_compaction_config
        assert config is not None
        assert config.compaction_interval == expected_interval
        assert config.overlap_size == expected_overlap
        assert config.token_threshold == expected_threshold
        assert config.event_retention_size == expected_retention


def test_app_events_compaction_config_env_vars():
    """Test app's events_compaction_config with custom env variables."""
    expected_interval = 5
    expected_overlap = 2
    expected_threshold = 600000
    expected_retention = 4

    with (
        patch.dict(
            "os.environ",
            {
                "CLIENT_ID": "test_client_id",
                "CLIENT_SECRET": "test_client_secret",
                "GOOGLE_ADS_DEVELOPER_TOKEN": "test_token",
                "COMPACTION_INTERVAL": str(expected_interval),
                "COMPACTION_OVERLAP_SIZE": str(expected_overlap),
                "COMPACTION_TOKEN_THRESHOLD": str(expected_threshold),
                "COMPACTION_EVENT_RETENTION_SIZE": str(expected_retention),
            },
        ),
        patch(
            "adspace_agent.agent.GoogleApiToOpenApiConverter.convert",
            return_value=DUMMY_OPENAPI,
        ),
    ):
        importlib.reload(agent_module)
        app = agent_module.app
        config = app.events_compaction_config
        assert config is not None
        assert config.compaction_interval == expected_interval
        assert config.overlap_size == expected_overlap
        assert config.token_threshold == expected_threshold
        assert config.event_retention_size == expected_retention
