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
"""Tools for the AdSpace Agent to interact with artifacts."""

from typing import override

from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import FunctionTool
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset
from google.adk.tools.tool_context import ToolContext


async def list_user_files(tool_context: ToolContext) -> str:
    """Tool to list available artifacts for the user.

    Args:
        tool_context: The context containing the artifact service.

    Returns:
        A string describing the available artifacts or an error message.
    """
    try:
        available_files = await tool_context.list_artifacts()
    except ValueError as e:
        print(f"Error listing artifacts: {e}. Is ArtifactService configured?")
        return "Error: Could not list artifacts."
    except Exception as e:  # noqa: BLE001
        print(f"An unexpected error occurred during artifact list: {e}")
        return "Error: An unexpected error occurred while listing artifacts."
    else:
        if not available_files:
            return "You have no saved artifacts."
        file_list_str = "\n".join([f"- {fname}" for fname in available_files])
        return f"Here are your available artifacts:\n{file_list_str}"


list_files_tool = FunctionTool(func=list_user_files)


class UtilitiesToolset(BaseToolset):
    """A custom toolset of utilities."""

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        return [list_files_tool]
