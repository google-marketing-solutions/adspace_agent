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
"""Tests for the data analysis tools."""

from unittest.mock import patch

from google.adk.agents import Agent
from google.adk.code_executors import BuiltInCodeExecutor
from google.adk.models.google_llm import Gemini
from google.adk.tools.agent_tool import AgentTool
import pytest

from adspace_agent.tools import data_analysis


def test_create_data_analysis_agent() -> None:
    """Tests that create_data_analysis_agent configures BuiltInCodeExecutor."""
    with patch.dict("os.environ", {"MODEL": "gemini-3.8-flash"}):
        agent = data_analysis.create_data_analysis_agent()
    assert isinstance(agent, Agent)
    assert agent.name == "data_analysis_agent"
    assert agent.model == Gemini(model="gemini-3.8-flash")
    assert isinstance(agent.code_executor, BuiltInCodeExecutor)


def test_data_analysis_tool() -> None:
    """Tests that data_analysis_tool wraps the sandboxed code agent."""
    tool = data_analysis.data_analysis_tool
    assert isinstance(tool, AgentTool)
    assert isinstance(tool.agent, Agent)
    assert isinstance(tool.agent.code_executor, BuiltInCodeExecutor)


@pytest.mark.asyncio
async def test_data_analysis_toolset() -> None:
    """Tests that the DataAnalysisToolset returns the correct tools."""
    toolset = data_analysis.DataAnalysisToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 1
    assert tools[0] is data_analysis.data_analysis_tool
