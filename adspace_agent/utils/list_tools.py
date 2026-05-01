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
"""Unified script to list tools for Google Ads, YouTube, and Google APIs."""

import argparse
import asyncio
import os
from typing import Any

from google.adk.tools.google_api_tool import GoogleApiToolset
from google.adk.tools.google_api_tool import YoutubeToolset
from google_ads_adk.discovery_converter import DiscoveryConverter
from google_ads_adk.toolset import GOOGLE_ADS_API_VERSION


def list_tools(tools: list[Any]) -> None:
    """Helper to sort and print tool names."""
    names = []
    for tool in tools:
        if hasattr(tool, "name"):
            names.append(tool.name)
        elif isinstance(tool, dict) and "name" in tool:
            names.append(tool["name"])
        else:
            names.append(str(tool))

    names.sort()
    for name in names:
        print(name)


def main() -> None:
    """Main function to parse arguments and list tools."""
    parser = argparse.ArgumentParser(
        description="List tools for various Google APIs."
    )
    subparsers = parser.add_subparsers(
        dest="type", help="Toolset type", required=True
    )

    # Ads subparser
    subparsers.add_parser("google_ads", help="List Google Ads tools.")

    # YouTube subparser
    subparsers.add_parser("youtube", help="List YouTube tools.")

    # Generic Google API subparser
    api_parser = subparsers.add_parser(
        "google", help="List generic Google API tools."
    )
    api_parser.add_argument(
        "--api", required=True, help="API name (e.g., drive, storage)"
    )
    api_parser.add_argument(
        "--version", required=True, help="API version (e.g., v3, v1)"
    )

    args = parser.parse_args()

    client_id = os.environ.get("CLIENT_ID", "dummy_client_id")
    client_secret = os.environ.get("CLIENT_SECRET", "dummy_client_secret")

    if args.type == "google_ads":
        converter = DiscoveryConverter(
            f"https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}"
        )
        spec = converter.convert()
        op_ids = []
        for methods in spec.get("paths", {}).values():
            for operation in methods.values():
                op_id = operation.get("operationId")
                if op_id:
                    op_ids.append(op_id)
        op_ids.sort()
        for op_id in op_ids:
            print(op_id)

    elif args.type == "youtube":
        toolset = YoutubeToolset(
            client_id=client_id,
            client_secret=client_secret,
        )
        tools = asyncio.run(toolset.get_tools())
        list_tools(tools)

    elif args.type == "google":
        toolset = GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name=args.api,
            api_version=args.version,
        )
        tools = asyncio.run(toolset.get_tools())
        list_tools(tools)


if __name__ == "__main__":  # pragma: no cover
    main()
