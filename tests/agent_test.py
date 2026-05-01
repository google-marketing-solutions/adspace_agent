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

from unittest.mock import patch

from google.adk.models.google_llm import Gemini
import pytest

from adspace_agent.agent import create_agent

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
    assert agent.model == Gemini(model="gemini-3.1-flash-lite-preview")


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
        "You are a helpful agent who can answer user questions about ads, "
        "creatives, data science, performance, analytics, and campaigns. NOTE: "
        "Tools for Search Ads 360 and tools for Google Ads are distinct. DO "
        "NOT group tools from one platform with tools from another platform "
        "if asked what tools are available. When asked for multiple pieces of "
        "information or to perform multiple actions, always try to call "
        "functions in parallel if possible."
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
