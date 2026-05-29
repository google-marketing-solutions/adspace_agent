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
"""Toolset for managing agent skills and their lifecycle."""

import os
import pathlib
from typing import override

from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.models import LlmRequest
from google.adk.skills import list_skills_in_dir
from google.adk.skills import list_skills_in_gcs_dir
from google.adk.skills import load_skill_from_dir
from google.adk.skills import load_skill_from_gcs_dir
from google.adk.skills import Skill
from google.adk.tools import FunctionTool
from google.adk.tools import skill_toolset
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset
from google.adk.tools.tool_context import ToolContext


def _load_skills_from_env() -> tuple[list[Skill], list[str]]:
    """Loads all skills from local or GCS directories based on env variables.

    Returns:
        A tuple containing:
          - A list of successfully loaded Skill objects.
          - A list of error messages for skills that failed to load.
    """
    skill_map: dict[str, Skill] = {}
    errors: list[str] = []

    if os.environ.get("SKILLS_BUCKET_NAME"):
        bucket_name = os.environ["SKILLS_BUCKET_NAME"]
        try:
            gcs_skills = list_skills_in_gcs_dir(bucket_name, "skills")
        except Exception as e:  # noqa: BLE001
            errors.append(
                f"Failed to list GCS skills in bucket '{bucket_name}': {e}"
            )
            gcs_skills = []

        for skill_id in gcs_skills:
            try:
                skill = load_skill_from_gcs_dir(
                    bucket_name=bucket_name,
                    skill_id=skill_id,
                    skills_base_path="skills",
                )
                skill_map[skill.name] = skill
            except Exception as e:  # noqa: BLE001
                errors.append(f"GCS skill '{skill_id}' failed to load: {e}")

    local_skills_dir = os.environ.get("LOCAL_SKILLS_DIR") or str(
        pathlib.Path(__file__).parent.parent.parent / "skills"
    )
    if pathlib.Path(local_skills_dir).exists():
        try:
            local_skills = list_skills_in_dir(local_skills_dir)
        except Exception as e:  # noqa: BLE001
            errors.append(
                f"Failed to list local skills in '{local_skills_dir}': {e}"
            )
            local_skills = []

        for skill_id in local_skills:
            try:
                new_skill = load_skill_from_dir(
                    str(pathlib.Path(local_skills_dir) / skill_id)
                )
                skill_map[new_skill.name] = new_skill
            except Exception as e:  # noqa: BLE001
                errors.append(f"Local skill '{skill_id}' failed to load: {e}")

    return list(skill_map.values()), errors


class SkillsToolset(BaseToolset):
    """A toolset for managing agent skills and their lifecycle."""

    def __init__(self) -> None:
        """Initializes the SkillsToolset and loads initial skills."""
        super().__init__()
        skills, errors = _load_skills_from_env()
        if errors:
            for err in errors:
                print(f"WARNING: {err}")
        self.skills_toolset = skill_toolset.SkillToolset(skills=skills)
        self.reload_skills_tool = FunctionTool(func=self.reload_skills)

    def reload_skills(self) -> str:
        """Reloads all skills from the configured local or GCS directories.

        Use this tool when you have updated, added, or modified any skills
        on the disk, to ensure you are using the latest definitions.

        Returns:
            A status message indicating the result of the reload.
        """
        new_skills, errors = _load_skills_from_env()
        self.skills_toolset._skills.clear()  # noqa: SLF001
        self.skills_toolset._skills.update(  # noqa: SLF001
            {s.name: s for s in new_skills}
        )

        skill_names = ", ".join([s.name for s in new_skills]) or "none"
        msg = f"Successfully reloaded skills. Available skills: {skill_names}."

        if errors:
            err_details = "\n".join([f"- {err}" for err in errors])
            msg += (
                "\n\nWARNING: The following errors occurred while loading "
                f"skills:\n{err_details}"
            )

        return msg

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        core_tools = await self.skills_toolset.get_tools(readonly_context)
        return [*core_tools, self.reload_skills_tool]

    @override
    async def process_llm_request(
        self, *, tool_context: ToolContext, llm_request: LlmRequest
    ) -> None:
        await self.skills_toolset.process_llm_request(
            tool_context=tool_context, llm_request=llm_request
        )

    @override
    async def close(self) -> None:
        await self.skills_toolset.close()
        await super().close()
