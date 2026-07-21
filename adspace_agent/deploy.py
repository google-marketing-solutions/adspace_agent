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
"""Deploy AdSpace to Agent Engine."""

import os
import pathlib
import typing

from dotenv import load_dotenv
from vertexai import agent_engines

from adspace_agent.agent import root_agent

load_dotenv()

import vertexai  # ruff:ignore[module-import-not-at-top-of-file]


def _load_requirements(path: pathlib.Path) -> list[str]:
    """Loads and parses a requirements.txt file.

    Args:
        path: The path to the requirements file.

    Returns:
        A list of package dependency specifications.
    """
    reqs = []
    with pathlib.Path(path).open(encoding="utf-8") as f:
        for line in f:
            stripped_line = line.strip()
            if not stripped_line or stripped_line.startswith(("#", "-")):
                continue
            cleaned_line = stripped_line.split("#")[0].strip()
            if cleaned_line:
                reqs.append(cleaned_line)
    return reqs


def main() -> None:
    """Main entry point to deploy the agent to Agent Engine."""
    vertexai.init(
        project=os.getenv("GOOGLE_CLOUD_PROJECT"),
        location="us-central1",
        staging_bucket="gs://adspace-agent",
    )

    app = agent_engines.AdkApp(
        agent=root_agent,
        enable_tracing=True,
        app_name="adspace-agent",
    )

    requirements = _load_requirements(
        pathlib.Path(__file__).parent.parent / "requirements.txt"
    )

    env_vars = {
        "GOOGLE_CLOUD_LOCATION": "global",
        "GOOGLE_GENAI_USE_VERTEXAI": "True",
        "CLIENT_ID": os.getenv("CLIENT_ID"),
        "CLIENT_SECRET": os.getenv("CLIENT_SECRET"),
        "GOOGLE_ADS_DEVELOPER_TOKEN": os.getenv("GOOGLE_ADS_DEVELOPER_TOKEN"),
        "GOOGLE_ADS_LOGIN_CUSTOMER_ID": os.getenv(
            "GOOGLE_ADS_LOGIN_CUSTOMER_ID"
        ),
        "MODEL": os.getenv("MODEL"),
        "VEO_MODEL": os.getenv("VEO_MODEL"),
        "IMAGEN_MODEL": os.getenv("IMAGEN_MODEL"),
        "ENABLED_TOOLSETS": os.getenv("ENABLED_TOOLSETS"),
        "GOOGLE_ADS_TOOL_FILTER": os.getenv("GOOGLE_ADS_TOOL_FILTER"),
        "SKILLS_BUCKET_NAME": os.getenv("SKILLS_BUCKET_NAME"),
        "LOCAL_SKILLS_DIR": os.getenv("LOCAL_SKILLS_DIR"),
    }
    env_vars = {k: v for k, v in env_vars.items() if v is not None}

    print("Checking for existing remote app on Agent Engine...")
    existing_engine = None
    try:
        for engine in agent_engines.list():
            if engine.display_name == "AdSpace Agent":
                existing_engine = engine
                break
    except Exception as e:  # ruff:ignore[blind-except]
        print(f"Error checking for existing reasoning engines: {e}")

    if existing_engine:
        print(f"Updating existing app: {existing_engine.resource_name}...")
        remote_app = agent_engines.update(
            resource_name=existing_engine.resource_name,
            agent_engine=typing.cast("typing.Any", app),
            requirements=requirements,
            extra_packages=["adspace_agent", "skills"],
            env_vars=typing.cast("typing.Any", env_vars),
            resource_limits={
                "cpu": "8",
                "memory": "32Gi",
            },
        )
    else:
        print("Creating new remote app on Agent Engine...")
        remote_app = agent_engines.create(
            display_name="AdSpace Agent",
            description=(
                "AdSpace Agent is designed to provide a standardized way to "
                "integrate an LLM with Google Ads, Display & Video 360, "
                "Campaign Manager 360, Search Ads 360, YouTube, Google "
                "Drive, and Google Cloud storage to form a more "
                "comprehensive campaign and marketing plan for agencies."
            ),
            agent_engine=typing.cast("typing.Any", app),
            requirements=requirements,
            extra_packages=["adspace_agent", "skills"],
            env_vars=typing.cast("typing.Any", env_vars),
            resource_limits={
                "cpu": "8",
                "memory": "32Gi",
            },
        )

    print(f"Deployment successful. Remote app ID: {remote_app.resource_name}")


if __name__ == "__main__":
    main()
