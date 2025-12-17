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
"""Tests for the utility tools."""

import pytest

from adspace_agent.tools import utilities


@pytest.mark.asyncio
async def test_utilities_toolset():
    """Tests that the UtilitiesToolset returns the correct tools."""
    toolset = utilities.UtilitiesToolset()
    tools = await toolset.get_tools()
    assert not tools
