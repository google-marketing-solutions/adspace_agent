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
"""Tools for the AdSpace Agent to interact with the REPL."""

import warnings

from google.adk.tools.langchain_tool import LangchainTool
from langchain_experimental.tools.python.tool import PythonAstREPLTool


class ExperimentalDataAnalysisToolkit(LangchainTool):

  """Uses a REPL agent to create code to analyze data.

  Example:

    experimental_data_analysis_toolkit = ExperimentalDataAnalysisToolkit()

    agent = Agent(
      name="agent_name",
      model="gemini-x.x-x",
      description="Agent to analyze data.",
      instruction=(
        "You are to only analyze data.  After you perform an analysis. "
        "After you do any analysis, you should show the code you wrote and "
        "the logic and reasoning for why you analyzed the data that way."
      ),
      tools=[
        experimental_data_analysis_toolkit
      ]
    )
  """

  def __init__(self):

    warnings.warn((
        "This agent is experimental and may not work as expected. "
        "The toolkit uses a REPL agent to write code, specifically "
        "Python code that can analyze data.  The agent may write incorrect "
        "code or not perform the appropriate statistical operations to analyze "
        "the data.  Use at your own risk."
    ))

    tool = PythonAstREPLTool()

    super().__init__(
        tool=tool,
        name="experimental_data_analysis_tool",
        description="Uses data science python stack to analyze data.",
    )
