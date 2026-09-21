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
"""ADK evaluation tests for AdSpace Agent."""

from collections.abc import AsyncGenerator
import os
import pathlib
from unittest.mock import patch

from google.adk.evaluation.agent_evaluator import AgentEvaluator
from google.adk.models.llm_request import LlmRequest
from google.adk.models.llm_response import LlmResponse
from google.genai import types
import pytest

EVAL_DIR = pathlib.Path(__file__).parent / "eval"


async def _mock_generate_content_async(  # ruff: ignore[unused-async]
    _self: object,
    llm_request: LlmRequest,
    stream: bool = False,  # ruff: ignore[unused-function-argument, boolean-type-hint-positional-argument, boolean-default-value-positional-argument]
) -> AsyncGenerator[LlmResponse, None]:
    """Deterministic LLM stub for offline ADK AgentEvaluator runs.

    Yields:
        LlmResponse: Simulated model tool calls and final responses.
    """
    last_content = llm_request.contents[-1] if llm_request.contents else None
    has_function_response = bool(
        last_content
        and last_content.parts
        and any(part.function_response for part in last_content.parts)
    )

    user_text = ""
    for content in reversed(llm_request.contents or []):
        if content.role == "user" and content.parts:
            for part in content.parts:
                if part.text:
                    user_text = part.text
                    break
        if user_text:
            break

    if not has_function_response:
        if "today's date" in user_text.lower():
            yield LlmResponse(
                content=types.Content(
                    role="model",
                    parts=[
                        types.Part(
                            function_call=types.FunctionCall(
                                id="call_get_current_date",
                                name="get_current_date",
                                args={},
                            )
                        )
                    ],
                )
            )
            return

        if "reload" in user_text.lower() and "skills" in user_text.lower():
            yield LlmResponse(
                content=types.Content(
                    role="model",
                    parts=[
                        types.Part(
                            function_call=types.FunctionCall(
                                id="call_reload_skills",
                                name="reload_skills",
                                args={},
                            )
                        )
                    ],
                )
            )
            return

    if "today's date" in user_text.lower():
        yield LlmResponse(
            content=types.Content(
                role="model",
                parts=[types.Part(text="Today's date is 2026-09-21.")],
            )
        )
        return

    yield LlmResponse(
        content=types.Content(
            role="model",
            parts=[
                types.Part(
                    text="I have reloaded the available skills from disk."
                )
            ],
        )
    )


@pytest.mark.asyncio
async def test_adspace_agent_evaluation() -> None:
    """Evaluates the AdSpace Agent using ADK AgentEvaluator."""
    if os.environ.get("ADK_RUN_LIVE_EVALS") == "1":  # pragma: no cover
        await AgentEvaluator.evaluate(
            agent_module="adspace_agent.agent",
            eval_dataset_file_path_or_dir=str(EVAL_DIR),
            num_runs=1,
        )
        return

    with patch(
        "google.adk.models.google_llm.Gemini.generate_content_async",
        new=_mock_generate_content_async,
    ):
        await AgentEvaluator.evaluate(
            agent_module="adspace_agent.agent",
            eval_dataset_file_path_or_dir=str(EVAL_DIR),
            num_runs=1,
        )
