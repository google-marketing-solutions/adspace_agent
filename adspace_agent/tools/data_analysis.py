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
"""Tools for the AdSpace Agent to interact with data analysis libraries."""

import os
from typing import override

from google.adk.agents import Agent
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.code_executors import BuiltInCodeExecutor
from google.adk.models.google_llm import Gemini
from google.adk.tools.agent_tool import AgentTool
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset

from adspace_agent.config import MODEL


def create_data_analysis_agent() -> Agent:
    """Creates a sub-agent configured with the built-in sandboxed code executor.

    Returns:
        Agent: The configured data analysis sub-agent.
    """
    model_name = os.environ.get("MODEL", MODEL)
    return Agent(
        name="data_analysis_agent",
        model=Gemini(model=model_name),
        description=(
            "An agent for performing data analysis and calculations using "
            "sandboxed built-in Python code execution."
        ),
        instruction=(
            "You are a specialized data analysis agent. Write and execute "
            "Python code using your built-in code executor to analyze data, "
            "perform calculations, and answer the given query."
        ),
        code_executor=BuiltInCodeExecutor(),
    )


data_analysis_tool = AgentTool(agent=create_data_analysis_agent())


class DataAnalysisToolset(BaseToolset):
    """A custom toolset that groups all our data analysis functions."""

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        return [data_analysis_tool]
