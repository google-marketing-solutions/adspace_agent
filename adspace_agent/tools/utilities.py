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
"""A set of utility tools for the AdSpace Agent to interact with."""

from typing import override

from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import BaseTool
from google.adk.tools.base_toolset import BaseToolset


class UtilitiesToolset(BaseToolset):
    """A custom toolset that groups all our utility functions."""

    @override
    async def get_tools(  # pytype: disable=override-error
        self,
        readonly_context: ReadonlyContext | None = None,  # pylint: disable=unused-argument
    ) -> list[BaseTool]:
        return []
