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

from typing import override

from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.integrations.langchain import LangchainTool
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset
from langchain_experimental.tools.python.tool import PythonAstREPLTool

python_ast_repl_tool = LangchainTool(
    name="python_repl_ast",
    description=(
        "A Python shell. Use this to execute python commands. Input should be "
        "a valid python command. When using this tool, sometimes output is "
        "abbreviated - make sure it does not look abbreviated before using it "
        "in your answer."
    ),
    tool=PythonAstREPLTool(),
)


class DataAnalysisToolset(BaseToolset):
    """A custom toolset that groups all our data analysis functions."""

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        return [python_ast_repl_tool]
