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
"""Testing the agent module."""

from unittest.mock import patch

from google.adk.models.google_llm import Gemini

with patch.dict(
    "os.environ",
    {
        "CLIENT_ID": "test_client_id",
        "CLIENT_SECRET": "test_client_secret",
    },
):
    from adspace_agent.agent import root_agent


def test_agent_name():
    """Test that the agent's name is correct."""
    assert root_agent.name == "adspace_agent"


def test_agent_model():
    """Test that the agent's model is correct."""
    assert root_agent.model == Gemini(model="gemini-3.1-pro-preview")


def test_agent_description():
    """Test that the agent's description is correct."""
    assert root_agent.description == (
        "AdSpace Agent is designed to provide a standardized way to integrate "
        "an LLM with Google Ads, YouTube, and Google Cloud to form a more "
        "comprehensive campaign and marketing plan for agencies."
    )


def test_agent_instruction():
    """Test that the agent's instruction is correct."""
    assert root_agent.instruction == (
        "You are a helpful agent who can answer user questions about ads, "
        "creatives, data science, performance, analytics, and campaigns."
    )
