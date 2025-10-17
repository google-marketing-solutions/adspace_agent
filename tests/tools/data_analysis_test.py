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
"""Tests for the data analysis tools."""

from typing import cast

import pytest

from adspace_agent.tools import data_analysis


def test_python_repl_tool():
    """Tests that the python_ast_repl_tool can execute code."""
    tool = data_analysis.python_ast_repl_tool
    result = cast(int, tool.func("1 + 1"))
    assert result == 2


@pytest.mark.asyncio
async def test_data_analysis_toolset():
    """Tests that the DataAnalysisToolset returns the correct tools."""
    toolset = data_analysis.DataAnalysisToolset()
    tools = await toolset.get_tools()
    assert len(tools) == 1
    assert tools[0] is data_analysis.python_ast_repl_tool
